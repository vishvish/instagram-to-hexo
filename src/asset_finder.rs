use std::hash::Hash;
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
use chrono::NaiveDateTime;
use dotenvy_macro::dotenv;
use glob::glob;
use lazy_static::lazy_static;
use log::{debug, error, info};
use regex::{Captures, Regex};

#[derive(Message)]
#[rtype(result = "Result<(HashMap<String, String>, Vec<String>), std::io::Error>")]
pub(crate) struct AssetMessage(pub HashMap<String, String>);

pub(crate) struct AssetFinder;

impl Actor for AssetFinder {
    type Context = Context<Self>;
}

impl Handler<AssetMessage> for AssetFinder {
    type Result = Result<(HashMap<String, String>, Vec<String>), std::io::Error>; // <- Message response type

    fn handle(&mut self, msg: AssetMessage, _ctx: &mut Context<Self>) -> Self::Result {
        let mut dd = msg.0;


        // find the correct list of media files, i.e. remove any images that have a corresponding video
        let (image_files, video_files) = find_media_files(dd.get("post_path").unwrap().to_string());

        let media_files = merge_media_file_lists(image_files, video_files);

        let mut media_files = media_files
            .into_iter()
            .map(|filename| {
                Path::new(dd.get("output_path").unwrap())
                    .join(filename)
                    .to_str()
                    .unwrap()
                    .to_string()
            })
            .collect::<Vec<String>>();
        //
        info!("received: {:?}", dd.get("post_path").unwrap());
        info!("final media_files: {:?}", media_files);

        Ok((dd, media_files))
    }
}

fn find_media_files(post_path: String) -> (Vec<String>, Vec<String>) {
    const INPUT_DIRECTORY: &str = dotenv!("INPUT_DIRECTORY");

    let mut image_files: Vec<String> = Vec::new();
    let mut video_files: Vec<String> = Vec::new();
    // find the core name of all the post files
    let post_stem = post_path.split(".").collect::<Vec<&str>>()[0];

    let image_pattern = format!("{}/**/{}*.{}", INPUT_DIRECTORY, post_stem, "jpg");
    for entry in glob(&image_pattern).expect("Failed to read glob pattern for jpeg") {
        match entry {
            Ok(path) => image_files.push(String::from(path.file_name().unwrap().to_str().unwrap())), // only the filename
            Err(e) => error!("Error: {:?}", e),
        }
    }

    let video_pattern = format!("{}/**/{}*.{}", INPUT_DIRECTORY, post_stem, "mp4");
    for entry in glob(&video_pattern).expect("Failed to read glob pattern for mp4") {
        match entry {
            Ok(path) => video_files.push(String::from(path.file_name().unwrap().to_str().unwrap())), // only the filename
            Err(e) => error!("Error: {:?}", e),
        }
    }
    (image_files, video_files)
}
fn merge_media_file_lists(a: Vec<String>, b: Vec<String>) -> Vec<String> {
    let b2 = b.clone();

    // Convert the vectors into vectors of PathBuf
    let image_files: Vec<PathBuf> = a.into_iter().map(PathBuf::from).collect();
    let video_files: Vec<PathBuf> = b.into_iter().map(PathBuf::from).collect();

    // Create a set of unique stems (without extensions) from video files
    let video_stems: std::collections::HashSet<String> = video_files
        .iter()
        .filter_map(|path| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .map(|stem_str| stem_str.to_string())
        })
        .collect();

    // Filter the image files to remove those with the same stem (without extensions) as any video file
    let cleaned_files: Vec<PathBuf> = image_files
        .into_iter()
        .filter(|image_path| {
            let image_stem = image_path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(|stem_str| stem_str.to_string())
                .unwrap_or(String::new());
            // Check if the image stem exists in the set of video stems
            !video_stems.contains(&image_stem)
        })
        .collect();

    // Convert the cleaned_files back to a vector of strings if needed
    let cleaned_files: Vec<String> = cleaned_files
        .iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect();

    // merge the cleaned image files with the video files
    let concat: Vec<String> = cleaned_files.into_iter().chain(b2.into_iter()).collect();
    concat
}
