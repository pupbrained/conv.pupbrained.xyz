use std::io::{Read, Seek};

use actix_cors::Cors;
use actix_multipart::form::{json::Json, tempfile::TempFile, MultipartForm};
use actix_web::{post, App, Error, HttpResponse, HttpServer, Responder, Result};
use jpeg_decoder::PixelFormat;

#[derive(Debug, MultipartForm)]
struct UploadForm {
  #[multipart(limit = "25MB")]
  file: TempFile,
  output_type: Json<String>,
}

enum Format {
  Png,
  Jpeg,
  WebP,
}

enum ColorType {
  Cmyk8,
  Grayscale16,
  Grayscale8,
  Indexed,
  Rgb8,
  Rgba8,
}

struct Decoded {
  bytes: Vec<u8>,
  color_type: ColorType,
  width: u32,
  height: u32,
}

impl Format {
  fn decode(&mut self, mut input: impl Read + Seek) -> Decoded {
    match self {
      Format::Png => {
        let decoder = png::Decoder::new(&mut input);

        let mut reader = decoder.read_info().expect("PNG: failed on read_info");

        let mut out = vec![0; reader.output_buffer_size()];

        let info = reader.next_frame(&mut out).expect("failed on next_frame");

        let bytes = &out[..info.buffer_size()];

        let width = reader.info().width;
        let height = reader.info().height;
        let color_type = match reader.info().color_type {
          png::ColorType::Grayscale => ColorType::Grayscale8,
          png::ColorType::Rgb => ColorType::Rgb8,
          png::ColorType::Indexed => ColorType::Indexed,
          png::ColorType::GrayscaleAlpha => ColorType::Grayscale16,
          png::ColorType::Rgba => ColorType::Rgba8
        };

        Decoded {
          bytes: bytes.to_vec(),
          color_type,
          width,
          height,
        }
      }
      Format::Jpeg => {
        let mut decoder = jpeg_decoder::Decoder::new(&mut input);

        decoder.read_info().expect("JPEG: failed on read_info");

        let info = decoder.info().expect("JPEG: failed to get info");

        let width = info.width as u32;
        let height = info.height as u32;
        let color_type = match info.pixel_format {
          PixelFormat::L8 => ColorType::Grayscale8,
          PixelFormat::L16 => ColorType::Grayscale16,
          PixelFormat::RGB24 => ColorType::Rgb8,
          PixelFormat::CMYK32 => ColorType::Cmyk8,
        };

        Decoded {
          bytes: decoder.decode().expect("JPEG: failed on decode"),
          color_type,
          width,
          height,
        }
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
          true => ColorType::Rgba8,
          false => ColorType::Rgb8,
        };

        decoder
          .read_image(&mut out)
          .expect("WebP: failed on read_image");

        Decoded {
          bytes: out,
          color_type,
          width,
          height,
        }
      }
    }
  }

  fn encode(&mut self, input: &[u8], width: u32, height: u32, color_type: ColorType) -> Vec<u8> {
    let mut out = Vec::new();
    match self {
      Format::Png => {
        let mut encoder = png::Encoder::new(&mut out, width, height);

        let png_color_type = match color_type {
          ColorType::Grayscale8 => png::ColorType::Grayscale,
          ColorType::Grayscale16 => png::ColorType::GrayscaleAlpha,
          ColorType::Rgb8 => png::ColorType::Rgb,
          ColorType::Indexed => png::ColorType::Indexed,
          ColorType::Rgba8 => png::ColorType::Rgba,
          ColorType::Cmyk8 => panic!("CMYK not supported"),
        };

        encoder.set_color(png_color_type);

        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(input).unwrap();
        writer.finish().unwrap();

        out
      }
      Format::Jpeg => {
        let encoder = jpeg_encoder::Encoder::new(&mut out, 100);

        let jpeg_color_type = match color_type {
          ColorType::Grayscale8 => jpeg_encoder::ColorType::Luma,
          ColorType::Grayscale16 => jpeg_encoder::ColorType::Luma,
          ColorType::Rgb8 => jpeg_encoder::ColorType::Rgb,
          ColorType::Indexed => panic!("Indexed not supported"),
          ColorType::Rgba8 => jpeg_encoder::ColorType::Rgba,
          ColorType::Cmyk8 => jpeg_encoder::ColorType::Cmyk
        };

        encoder
          .encode(
            input,
            width.try_into().unwrap(),
            height.try_into().unwrap(),
            jpeg_color_type
          )
          .unwrap();

        out
      }
      Format::WebP => {
        let encoder = image_webp::WebPEncoder::new(&mut out);

        let webp_color_type = match color_type {
          ColorType::Grayscale8 => image_webp::ColorType::L8,
          ColorType::Grayscale16 => image_webp::ColorType::La8,
          ColorType::Rgb8 => image_webp::ColorType::Rgb8,
          ColorType::Rgba8 => image_webp::ColorType::Rgba8,
          _ => panic!("Unsupported color type")
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
) -> Result<impl Responder, Error> {
  let file = std::io::BufReader::new(input.file.into_file());

  let decoded = match input.content_type.clone().unwrap().subtype().as_str() {
    "png" => Format::Png.decode(file),
    "webp" => Format::WebP.decode(file),
    "jpeg" => Format::Jpeg.decode(file),
    _ => return Ok(HttpResponse::BadRequest().body("Unsupported input type")),
  };

  let Decoded {
    bytes,
    color_type,
    width,
    height,
  } = decoded;

  let out = match input.content_type.clone().unwrap().subtype().as_str() {
    "png" => Format::Png.encode(&bytes, width, height, color_type),
    "jpeg" => Format::Jpeg.encode(&bytes, width, height, color_type),
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
