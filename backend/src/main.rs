use std::io::Read;

use actix_cors::Cors;
use actix_multipart::form::{json::Json, tempfile::TempFile, MultipartForm};
use actix_web::{web, App, Error, HttpResponse, HttpServer, Responder, Result};

use icu_lib::{
  endecoder::{common::*, EnDecoder},
  midata,
  EncoderParams,
};

#[derive(Debug, MultipartForm)]
struct UploadForm {
  #[multipart(limit = "25MB")]
  file: TempFile,
  output_type: Json<String>,
}

async fn convert_image(
  MultipartForm(UploadForm {
    file,
    output_type,
  }): MultipartForm<UploadForm>,
) -> Result<impl Responder, Error> {
  let mut buf = Vec::new();
  file.file.as_file().read_to_end(&mut buf)?;

  // Determine input type
  let input_encoder: Box<dyn EnDecoder> = match file.content_type.unwrap().subtype().as_str() {
    "webp" => Box::new(WEBP {}),
    "png" => Box::new(PNG {}),
    "jpeg" => Box::new(JPEG {}),
    "bmp" => Box::new(BMP {}),
    "gif" => Box::new(GIF {}),
    "tiff" => Box::new(TIFF {}),
    "ico" => Box::new(ICO {}),
    "pbm" => Box::new(PBM {}),
    "pgm" => Box::new(PGM {}),
    "ppm" => Box::new(PPM {}),
    "pam" => Box::new(PAM {}),
    "tga" => Box::new(TGA {}),
    _ => return Ok(HttpResponse::BadRequest().body("Unsupported input type")),
  };

  let mid = midata::MiData::decode_from(&*input_encoder, buf);

  // Determine output type
  let output_encoder: Box<dyn EnDecoder> = match output_type.to_lowercase().as_str() {
    "bmp" => Box::new(BMP {}),
    "gif" => Box::new(GIF {}),
    "ico" => Box::new(ICO {}),
    "jpeg" => Box::new(JPEG {}),
    "pam" => Box::new(PAM {}),
    "pbm" => Box::new(PBM {}),
    "pgm" => Box::new(PGM {}),
    "png" => Box::new(PNG {}),
    "ppm" => Box::new(PPM {}),
    "tga" => Box::new(TGA {}),
    "tiff" => Box::new(TIFF {}),
    "webp" => Box::new(WEBP {}),
    _ => return Ok(HttpResponse::BadRequest().body("Unsupported output type")),
  };

  let data = mid.encode_into(&*output_encoder, EncoderParams::default());

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
      .body(data),
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

    App::new()
      .wrap(cors)
      .route("/convert_image", web::post().to(convert_image))
  })
  .bind("127.0.0.1:8080")?
  .run()
  .await
}
