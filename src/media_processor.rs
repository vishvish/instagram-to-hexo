use std::{
    any::Any,
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::{exit, id},
};

use actix::{
    Actor, Context, ContextFutureSpawner, Handler, Message, ResponseActFuture, System, WrapFuture,
};
use actix::fut::result;
use async_std::{net::TcpListener, net::TcpStream, task};
use chrono::NaiveDateTime;
use dotenvy_macro::dotenv;
use futures::executor::block_on;
use futures::future::join_all;
use glob::glob;
use lazy_static::lazy_static;
use log::{debug, error, info};
use photon_rs::multiple::watermark;
use photon_rs::native::open_image;
use photon_rs::native::save_image;
use photon_rs::PhotonImage;
use photon_rs::transform::{crop, resize, SamplingFilter};
use regex::{Captures, Regex};
use serde::Serialize;

use crate::post_actor;

#[derive(Message)]
#[rtype(result = "Result<HashMap<String, String>, std::io::Error>")]
pub(crate) struct MediaMessage(pub String);

pub(crate) struct MediaProcessor;

impl Actor for MediaProcessor {
    type Context = Context<Self>;
}

impl Handler<MediaMessage> for MediaProcessor {
    type Result = Result<HashMap<String, String>, std::io::Error>; // <- Message response type

    fn handle(&mut self, msg: MediaMessage, _ctx: &mut Context<Self>) -> Self::Result {
        let filepath = msg.0.clone();
        let output: Vec<String> = block_on(generate_images(filepath));
        let mut images: HashMap<String, String> = HashMap::new();
        images.insert("image".to_string(), output[0].clone());
        images.insert("thumbnail_image".to_string(), output[1].clone());
        Ok(images)
    }
}

async fn generate_images(filepath: String) -> Vec<String> {
    const INPUT_DIRECTORY: &str = dotenv!("INPUT_DIRECTORY");

    let mut path: PathBuf = PathBuf::from(&filepath);
    let filename = &path.file_name().unwrap().to_str().unwrap();
    let input_file: PathBuf = Path::new(&INPUT_DIRECTORY).join(&filename);
    let infile = input_file.clone();
    let original_filepath = filepath.clone();

    let tasks = vec![
        task::spawn(process_images(original_filepath.clone(), input_file)),
        task::spawn(process_thumbnail(original_filepath.clone(), path, infile)),
    ];
    // return the strings
    join_all(tasks).await
}

async fn process_images(filepath: String, input_file: PathBuf) -> String {
    const OUTPUT_DIRECTORY: &str = dotenv!("OUTPUT_DIRECTORY");
    const WATERMARK_IMG: &str = dotenv!("WATERMARK_IMG");
    const LARGE_IMAGE_DIMENSIONS_WIDTH: &str = dotenv!("LARGE_IMAGE_DIMENSIONS_WIDTH");
    const LARGE_IMAGE_DIMENSIONS_HEIGHT: &str = dotenv!("LARGE_IMAGE_DIMENSIONS_HEIGHT");

    let dt = post_actor::make_headings_from_filepath(filepath.clone());

    let output_directory = format!("{}/img/instagram/{}", dotenv!("OUTPUT_DIRECTORY"), dt.4);

    let img_path: String = input_file.to_str().unwrap().to_string();

    let mut img: PhotonImage = open_image(&img_path).expect("Image should open");
    let mark: PhotonImage = open_image(WATERMARK_IMG).expect("Watermark should open");

    // watermark placement
    // TODO: fix possible overflow error with subtraction
    let right_placement = img.get_width() - mark.get_width() - 30_u32;
    let bottom_placement = img.get_height() - mark.get_height() - 40_u32;
    // resize image before watermark
    let mut large_image: PhotonImage = resize(
        &img,
        LARGE_IMAGE_DIMENSIONS_WIDTH.parse().unwrap(),
        LARGE_IMAGE_DIMENSIONS_HEIGHT.parse().unwrap(),
        SamplingFilter::Lanczos3,
    );

    // watermark image
    watermark(&mut img, &mark, right_placement, bottom_placement);

    // set up naming for large image
    let output_path = format!("{}/{}", &output_directory, filepath);

    match save_image(img, output_path.as_str()) {
        Ok(_) => info!("Large image saved successfully: {}", output_path),
        Err(e) => error!("Error saving image: {:?}", e),
    }

    output_path.clone().to_string()
}

async fn process_thumbnail(filepath: String, path: PathBuf, infile: PathBuf) -> String {
    const THUMBNAIL_IMAGE_DIMENSIONS_WIDTH: &str = dotenv!("THUMBNAIL_IMAGE_DIMENSIONS_WIDTH");
    const THUMBNAIL_IMAGE_DIMENSIONS_HEIGHT: &str = dotenv!("THUMBNAIL_IMAGE_DIMENSIONS_HEIGHT");

    const OUTPUT_DIRECTORY: &str = dotenv!("OUTPUT_DIRECTORY");

    let mut thumbnail_img = open_image(infile.to_str().unwrap()).expect("Image should open");
    let mut crop_x = 0;
    let mut crop_y = 0;
    let mut crop_width = 0;
    let mut crop_height = 0;

    // Get the dimensions of the image
    let width = thumbnail_img.get_width();
    let height = thumbnail_img.get_height();

    match width > height {
        true => {
            debug!("landscape");
            let middle = height / 2;
            crop_x = middle - (height / 2);
            crop_width = height;
            crop_height = height;
        }
        false => {
            debug!("portrait");
            let middle = width / 2;
            crop_y = middle - (width / 2);
            crop_width = width;
            crop_height = width;
        }
    }
    let mut cropped_img = crop(&mut thumbnail_img, crop_x, crop_y, crop_width, crop_height);

    let result_image: PhotonImage = resize(
        &cropped_img,
        THUMBNAIL_IMAGE_DIMENSIONS_WIDTH.parse().unwrap(),
        THUMBNAIL_IMAGE_DIMENSIONS_HEIGHT.parse().unwrap(),
        SamplingFilter::Nearest,
    );

    let dt = post_actor::make_headings_from_filepath(filepath.clone());
    let output_directory = format!("{}/img/instagram/{}", dotenv!("OUTPUT_DIRECTORY"), dt.4);

    // // set up naming for thumbnail image
    let file_stem = Path::new(&filepath).file_stem().unwrap().to_str().unwrap();
    let extension = Path::new(&filepath).extension().unwrap().to_str().unwrap();
    let parent = path.parent().unwrap().to_str().unwrap();
    let thumbnail_output_path = format!(
        "{}/{}_thumb.{}",
        output_directory, file_stem, extension
    );

    match save_image(result_image, &thumbnail_output_path) {
        Ok(_) => info!("Thumbnail saved successfully: {}", thumbnail_output_path),
        Err(e) => error!("Error saving thumbnail: {:?}", e),
    }

    Path::new(&thumbnail_output_path)
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string()
}
