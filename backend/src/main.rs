use std::{
  fmt::Display,
  io::{self, BufRead, Seek, SeekFrom},
  panic,
};

use actix_cors::Cors;
use actix_multipart::form::{json::Json, tempfile::TempFile, MultipartForm};
use actix_web::{post, App, HttpResponse, HttpServer, Responder};
use anyhow::Context;
use jpegxl_rs::{
  decode::{Metadata, Pixels},
  encode::{EncoderResult, EncoderSpeed},
};
use thiserror::Error;

#[derive(Debug, MultipartForm)]
struct UploadForm {
  #[multipart(limit = "25MB")]
  file: TempFile,
  output_type: Json<String>,
}

#[derive(Debug)]
enum Format {
  Png,
  Jpeg,
  Jxl,
  WebP,
}

impl Display for Format {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Format::Png => write!(f, "PNG"),
      Format::Jpeg => write!(f, "JPEG"),
      Format::Jxl => write!(f, "JPEG-XL"),
      Format::WebP => write!(f, "WebP"),
    }
  }
}

#[derive(Debug)]
enum ColorType {
  Cmyk,
  GrayscaleAlpha,
  Grayscale,
  Rgb,
  Rgba,
  YCbCr,
}

impl Display for ColorType {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      ColorType::Cmyk => write!(f, "CMYK"),
      ColorType::Grayscale => write!(f, "Gray"),
      ColorType::GrayscaleAlpha => write!(f, "Grayscale (w/ Alpha)"),
      ColorType::Rgb => write!(f, "RGB"),
      ColorType::Rgba => write!(f, "RGBA"),
      ColorType::YCbCr => write!(f, "YCbCr"),
    }
  }
}

#[derive(Error, Debug)]
enum Error {
  #[error("Could not read info from {0} file")]
  CouldNotReadInfo(Format),
  // #[error("{0}: Unsupported color type {1}")]
  // UnsupportedColorType(Format, String),
  #[error("Could not get next frame")]
  NextFrameNotFound,
}

struct Decoded {
  bytes: Vec<u8>,
  color_type: ColorType,
  width: u32,
  height: u32,
}

impl Format {
  fn decode(&mut self, mut input: impl BufRead + Seek) -> anyhow::Result<Decoded> {
    match self {
      Format::Png => {
        let decoder = png::Decoder::new(&mut input);

        let mut reader = decoder
          .read_info()
          .context(Error::CouldNotReadInfo(Format::Png))?;

        let mut out = vec![0; reader.output_buffer_size()];

        let info = reader
          .next_frame(&mut out)
          .context(Error::NextFrameNotFound)?;

        let bytes = &out[..info.buffer_size()];

        let width = reader.info().width;
        let height = reader.info().height;

        let color_type = match reader.info().color_type {
          png::ColorType::Grayscale => ColorType::Grayscale,
          png::ColorType::GrayscaleAlpha => ColorType::GrayscaleAlpha,
          png::ColorType::Rgb => ColorType::Rgb,
          png::ColorType::Rgba => ColorType::Rgba,

          c => panic!("PNG: unsupported color type: {:?}", c),
        };

        Ok(Decoded {
          bytes: bytes.to_vec(),
          color_type,
          width,
          height,
        })
      }
      Format::Jpeg => {
        let decoder = mozjpeg::Decompress::builder()
          .from_reader(&mut input)
          .expect("Could not build JPEG decompressor");

        let width = decoder.width() as u32;
        let height = decoder.height() as u32;
        let color_space = decoder.color_space();

        let color_type = match color_space {
          mozjpeg::ColorSpace::JCS_GRAYSCALE => ColorType::Grayscale,
          mozjpeg::ColorSpace::JCS_RGB => ColorType::Rgb,
          mozjpeg::ColorSpace::JCS_YCbCr => ColorType::YCbCr,
          mozjpeg::ColorSpace::JCS_CMYK => ColorType::Cmyk,
          e => panic!("JPEG: unsupported color type: {:?}", e),
        };

        let mut pixels = decoder.to_colorspace(color_space).expect("weh");

        let bytes = pixels
          .read_scanlines()
          .expect("Could not read JPEG scanlines");

        pixels
          .finish()
          .expect("Could not finish JPEG decompression");

        Ok(Decoded {
          bytes,
          color_type,
          width,
          height,
        })
      }
      Format::Jxl => {
        fn convert_bufread_to_vec<R: BufRead + Seek>(reader: &mut R) -> io::Result<Vec<u8>> {
          // First, seek to the beginning to ensure we read from the start
          reader.seek(SeekFrom::Start(0))?;

          // Create a Vec<u8> to hold the data
          let mut buffer = Vec::new();

          // Read all data from the reader into the buffer
          reader.read_to_end(&mut buffer)?;

          // Return the buffer
          Ok(buffer)
        }

        let decoder = jpegxl_rs::decoder_builder()
          .build()
          .expect("JXL: failed on new");

        let input_as_u8 =
          convert_bufread_to_vec(&mut input).expect("Failed on convert_bufread_to_vec");

        let (
          Metadata {
            width,
            height,
            num_color_channels,
            has_alpha_channel,
            ..
          },
          pixels,
        ) = decoder.decode(&input_as_u8).expect("Failed on decode");

        let color_type = match (num_color_channels, has_alpha_channel) {
          (1, false) => ColorType::Grayscale,
          (1, true) => ColorType::GrayscaleAlpha,
          (3, false) => ColorType::Rgb,
          (3, true) => ColorType::Rgba,
          (4, false) => ColorType::Rgba,
          (4, true) => ColorType::Rgba,
          _ => panic!("JXL: unsupported color type"),
        };

        let bytes = match pixels {
          Pixels::Uint8(data) => data.to_vec(),
          _ => panic!("JXL: unsupported pixel type"),
        };

        Ok(Decoded {
          bytes,
          width,
          height,
          color_type,
        })
      }
      Format::WebP => {
        let mut decoder = image_webp::WebPDecoder::new(&mut input).expect("WebP: failed on new");

        let mut out = vec![
          0;
          decoder
            .output_buffer_size()
            .expect("WebP: failed to get buffer size")
        ];

        let (width, height) = decoder.dimensions();
        let color_type = match decoder.has_alpha() {
          true => ColorType::Rgba,
          false => ColorType::Rgb,
        };

        decoder
          .read_image(&mut out)
          .expect("WebP: failed on read_image");

        Ok(Decoded {
          bytes: out,
          color_type,
          width,
          height,
        })
      }
    }
  }

  fn encode(&mut self, input: &[u8], width: u32, height: u32, color_type: ColorType) -> Vec<u8> {
    let mut out = Vec::new();

    match self {
      Format::Png => {
        let mut encoder = png::Encoder::new(&mut out, width, height);

        let png_color_type = match color_type {
          ColorType::Grayscale => png::ColorType::Grayscale,
          ColorType::GrayscaleAlpha => png::ColorType::GrayscaleAlpha,
          ColorType::Rgb => png::ColorType::Rgb,
          ColorType::Rgba => png::ColorType::Rgba,
          c => panic!("PNG: unsupported color type: {:?}", c),
        };

        encoder.set_color(png_color_type);

        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(input).unwrap();
        writer.finish().unwrap();

        out
      }
      Format::Jpeg => {
        let color_space = match color_type {
          ColorType::Cmyk => mozjpeg::ColorSpace::JCS_CMYK,
          ColorType::Grayscale => mozjpeg::ColorSpace::JCS_GRAYSCALE,
          ColorType::Rgb => mozjpeg::ColorSpace::JCS_RGB,
          ColorType::YCbCr => mozjpeg::ColorSpace::JCS_YCbCr,
          _ => unimplemented!(),
        };

        let mut encoder = mozjpeg::Compress::new(color_space);

        encoder.set_size(width as usize, height as usize);

        let mut comp = encoder
          .start_compress(out)
          .expect("JPEG: failed on start_compress");

        comp
          .write_scanlines(input)
          .expect("Failed on write_scanlines");

        comp.finish().expect("Failed on finish")
      }
      Format::Jxl => {
        let mut encoder = jpegxl_rs::encoder_builder()
          .build()
          .expect("JXL: failed on new");

        encoder.speed = EncoderSpeed::Lightning;

        let buffer: EncoderResult<u8> = encoder
          .encode(input, width, height)
          .expect("JXL: failed on encode");

        buffer.to_vec()
      }
      Format::WebP => {
        let encoder = image_webp::WebPEncoder::new(&mut out);

        let webp_color_type = match color_type {
          ColorType::Grayscale => image_webp::ColorType::L8,
          ColorType::GrayscaleAlpha => image_webp::ColorType::La8,
          ColorType::Rgb => image_webp::ColorType::Rgb8,
          ColorType::Rgba => image_webp::ColorType::Rgba8,
          _ => panic!("Unsupported color type"),
        };

        encoder
          .encode(input, width, height, webp_color_type)
          .unwrap();

        out
      }
    }
  }
}

#[post("/convert_image")]
async fn convert_image(
  MultipartForm(UploadForm {
    file: input,
    output_type,
  }): MultipartForm<UploadForm>,
) -> actix_web::Result<impl Responder, actix_web::Error> {
  let file = std::io::BufReader::new(input.file.into_file());

  let decoded = match input.content_type.clone().unwrap().subtype().as_str() {
    "png" => Format::Png.decode(file),
    "jpeg" => Format::Jpeg.decode(file),
    "jxl" => Format::Jxl.decode(file),
    "webp" => Format::WebP.decode(file),

    _ => return Ok(HttpResponse::BadRequest().body("Unsupported input type")),
  };

  let Decoded {
    bytes,
    color_type,
    width,
    height,
  } = decoded.unwrap();

  let out = match input.content_type.clone().unwrap().subtype().as_str() {
    "png" => Format::Png.encode(&bytes, width, height, color_type),
    "jpeg" => Format::Jpeg.encode(&bytes, width, height, color_type),
    "jxl" => Format::Jxl.encode(&bytes, width, height, color_type),
    "webp" => Format::WebP.encode(&bytes, width, height, color_type),

    _ => return Ok(HttpResponse::BadRequest().body("Unsupported output type")),
  };

  Ok(
    HttpResponse::Ok()
      .content_type(match output_type.to_lowercase().as_str() {
        "bmp" => "image/bmp",
        "gif" => "image/gif",
        "ico" => "image/x-icon",
        "jpeg" => "image/jpeg",
        "pbm" | "pgm" | "ppm" | "pam" => "image/x-portable-anymap",
        "png" => "image/png",
        "tga" => "image/x-tga",
        "tiff" => "image/tiff",
        "webp" => "image/webp",
        _ => "application/octet-stream",
      })
      .body(out),
  )
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
  HttpServer::new(|| {
    let cors = Cors::default()
      .allow_any_origin()
      .allowed_methods(vec!["POST"])
      .allowed_header(actix_web::http::header::CONTENT_TYPE)
      .max_age(3600);

    App::new().wrap(cors).service(convert_image)
  })
  .bind("127.0.0.1:8080")?
  .run()
  .await
}
