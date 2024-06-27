use std::io::{Read, Seek};

use actix_cors::Cors;
use actix_multipart::form::{json::Json, tempfile::TempFile, MultipartForm};
use actix_web::{post, App, Error, HttpResponse, HttpServer, Responder, Result};

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

struct Decoded {
  bytes: Vec<u8>,
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

        Decoded {
          bytes: bytes.to_vec(),
          width,
          height,
        }
      }
      Format::Jpeg => {
        let mut decoder = jpeg_decoder::Decoder::new(&mut input);

        decoder.read_info().expect("JPEG: failed on read_info");

        let width = decoder.info().expect("JPEG: failed to get width").width as u32;
        let height = decoder.info().expect("JPEG: failed to get height").height as u32;

        Decoded {
          bytes: decoder.decode().expect("JPEG: failed on decode"),
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

        decoder
          .read_image(&mut out)
          .expect("WebP: failed on read_image");

        Decoded {
          bytes: out,
          width,
          height,
        }
      }
    }
  }

  fn encode(&mut self, input: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::new();
    match self {
      Format::Png => {
        let encoder = png::Encoder::new(&mut out, width, height);

        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(input).unwrap();
        writer.finish().unwrap();

        out
      }
      Format::Jpeg => {
        let encoder = jpeg_encoder::Encoder::new(&mut out, 100);

        encoder
          .encode(
            input,
            width.try_into().unwrap(),
            height.try_into().unwrap(),
            jpeg_encoder::ColorType::Rgb,
          )
          .unwrap();

        out
      }
      Format::WebP => {
        let encoder = image_webp::WebPEncoder::new(&mut out);

        encoder
          .encode(input, width, height, image_webp::ColorType::Rgba8)
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
    width,
    height,
  } = decoded;

  let out = match input.content_type.clone().unwrap().subtype().as_str() {
    "png" => Format::Png.encode(&bytes, width, height),
    "jpeg" => Format::Jpeg.encode(&bytes, width, height),
    "webp" => Format::WebP.encode(&bytes, width, height),

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
  ffmpeg_next::init()?;

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
