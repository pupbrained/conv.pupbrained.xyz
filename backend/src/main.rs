use std::{
    fmt::Display,
    io::{BufRead, Seek},
};

use actix_cors::Cors;
use actix_multipart::form::{json::Json, tempfile::TempFile, MultipartForm};
use actix_web::{post, App, HttpResponse, HttpServer, Responder};
use anyhow::{bail, Context};
use aom_decode::Config;
use ravif::Img;
use rgb::{ComponentMap, FromSlice};
use thiserror::Error;

#[derive(Debug, MultipartForm)]
struct UploadForm {
    #[multipart(limit = "25MB")]
    file: TempFile,
    output_type: Json<String>,
}

#[derive(Debug)]
enum Format {
    Avif,
    Png,
    Jpeg,
    WebP,
}

impl Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use Format::*;

        match self {
            Avif => write!(f, "AVIF"),
            Png => write!(f, "PNG"),
            Jpeg => write!(f, "JPEG"),
            WebP => write!(f, "WebP"),
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

#[derive(Debug)]
struct Decoded {
    bytes: Vec<u8>,
    color_type: ColorType,
    width: u32,
    height: u32,
}

#[derive(Error, Debug)]
enum Error {
    #[error("Could not read info from {0} file")]
    CouldNotReadInfo(Format),
    #[error("{0}: Unsupported color type {1}")]
    UnsupportedColorType(Format, String),
    #[error("Could not get next frame")]
    NextFrameNotFound,
}

impl Format {
    fn decode(&mut self, mut input: impl BufRead + Seek) -> anyhow::Result<Decoded> {
        match self {
            Format::Avif => {
                use aom_decode::avif::Image::*;

                let mut buf = Vec::new();

                input
                    .read_to_end(&mut buf)
                    .expect("Failed to read AVIF file");

                let mut decoder = aom_decode::avif::Avif::decode(
                    &buf,
                    &Config {
                        threads: num_cpus::get(),
                    },
                )
                .expect("Could not read AVIF");

                match decoder.convert().expect("Failed to convert") {
                    RGB8(img) => {
                        let (out, width, height) = img.into_contiguous_buf();

                        Ok(Decoded {
                            bytes: out.iter().flat_map(|x| [x.r, x.g, x.b]).collect(),
                            color_type: ColorType::Rgb,
                            width: width as u32,
                            height: height as u32,
                        })
                    }
                    RGBA8(img) => {
                        let (out, width, height) = img.into_contiguous_buf();

                        Ok(Decoded {
                            bytes: out.iter().flat_map(|x| [x.r, x.g, x.b, x.a]).collect(),
                            color_type: ColorType::Rgba,
                            width: width as u32,
                            height: height as u32,
                        })
                    }
                    Gray8(img) => {
                        let (out, width, height) = img.into_contiguous_buf();

                        Ok(Decoded {
                            bytes: out.to_vec(),
                            color_type: ColorType::Grayscale,
                            width: width as u32,
                            height: height as u32,
                        })
                    }
                    RGB16(img) => {
                        let mut out = Vec::new();

                        for px in img.pixels() {
                            out.push(px.map(|c| (c >> 8) as u8));
                        }

                        Ok(Decoded {
                            bytes: out.iter().flat_map(|x| [x.r, x.g, x.b]).collect(),
                            color_type: ColorType::Rgb,
                            width: img.width() as u32,
                            height: img.height() as u32,
                        })
                    }
                    RGBA16(img) => {
                        let mut out = Vec::new();

                        for px in img.pixels() {
                            out.push(px.map(|c| (c >> 8) as u8));
                        }

                        Ok(Decoded {
                            bytes: out.iter().flat_map(|x| [x.r, x.g, x.b, x.a]).collect(),
                            color_type: ColorType::Rgba,
                            width: img.width() as u32,
                            height: img.height() as u32,
                        })
                    }
                    Gray16(img) => {
                        let mut out = Vec::new();

                        for px in img.pixels() {
                            out.push((px >> 8) as u8);
                        }

                        Ok(Decoded {
                            bytes: out.to_vec(),
                            color_type: ColorType::Grayscale,
                            width: img.width() as u32,
                            height: img.height() as u32,
                        })
                    }
                }
            }
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

                    c => bail!(Error::UnsupportedColorType(Format::Png, format!("{c:?}"))),
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

                    e => bail!(Error::UnsupportedColorType(Format::Jpeg, format!("{e:?}"))),
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
            Format::WebP => {
                let mut decoder =
                    image_webp::WebPDecoder::new(&mut input).expect("WebP: failed on new");

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
            Format::Avif => {
                let encoder = ravif::Encoder::new()
                    .with_quality(95.)
                    .with_speed(10)
                    .encode_rgba(Img::new(input.as_rgba(), width as usize, height as usize))
                    .expect("Failed to encode to AVIF");

                encoder.avif_file
            }
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
                    ColorType::Rgba => mozjpeg::ColorSpace::JCS_EXT_RGBA,
                    ColorType::YCbCr => mozjpeg::ColorSpace::JCS_YCbCr,
                    c => panic!("Unsupported color type: {:?}", c),
                };

                let mut encoder = mozjpeg::Compress::new(color_space);

                encoder.set_quality(95.0);
                encoder.set_size(width as usize, height as usize);

                let mut comp = encoder
                    .start_compress(out)
                    .expect("JPEG: failed on start_compress");

                comp.write_scanlines(input)
                    .expect("Failed on write_scanlines");

                comp.finish().expect("Failed on finish")
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
        "avif" => Format::Avif.decode(file),
        "png" => Format::Png.decode(file),
        "jpeg" => Format::Jpeg.decode(file),
        "webp" => Format::WebP.decode(file),

        _ => return Ok(HttpResponse::BadRequest().body("Unsupported input type")),
    };

    let Decoded {
        bytes,
        color_type,
        width,
        height,
    } = decoded.unwrap();

    let out = match output_type.as_str() {
        "avif" => Format::Avif.encode(&bytes, width, height, color_type),
        "png" => Format::Png.encode(&bytes, width, height, color_type),
        "jpeg" => Format::Jpeg.encode(&bytes, width, height, color_type),
        "webp" => Format::WebP.encode(&bytes, width, height, color_type),

        _ => return Ok(HttpResponse::BadRequest().body("Unsupported output type")),
    };

    Ok(HttpResponse::Ok()
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
        .body(out))
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
