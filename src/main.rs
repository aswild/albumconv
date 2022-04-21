use std::borrow::Cow;
use std::io::{stdout, Write};
use std::path::PathBuf;
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use deunicode::deunicode;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Track {
    file: PathBuf,
    disc: u32,
    track: u32,
    title: String,
    artist: String,
}

#[derive(Debug, Parser)]
struct Args {
    /// Cover art file (jpg or png image)
    #[clap(short, long)]
    cover: Option<PathBuf>,

    /// Directory that input files are loacted in
    #[clap(short = 'd', long)]
    input_dir: Option<PathBuf>,

    #[clap(short = 't', long)]
    album_title: Option<String>,
    #[clap(short, long)]
    album_artist: Option<String>,
    #[clap(short = 'y', long)]
    date: Option<String>,

    /// CSV file containing columns: file, disc, track, title, artist
    input_csv: PathBuf,

    /// Directory to write output files
    output_dir: PathBuf,
}

fn maybe_metadata(key: &str, val: &Option<String>) -> String {
    match val {
        Some(ref val) => format!("{key}={val}"),
        None => String::new(),
    }
}

impl Args {
    fn convert_track(&self, track: &Track) -> Result<(), PathBuf> {
        let input_file = match &self.input_dir {
            Some(dir) => Cow::Owned(dir.join(&track.file)),
            None => Cow::Borrowed(&track.file),
        };

        let output_file = self.output_dir.join(format!(
            "{disc}.{track:02}-{artist}-{title}.flac",
            disc = track.disc,
            track = track.track,
            artist = deunicode(&track.artist),
            title = deunicode(&track.title),
        ));

        let mut cmd = Command::new("ffmpeg");
        cmd.args(&["-hide_banner", "-nostdin", "-i"]);
        cmd.arg(&*input_file);
        if let Some(cover) = &self.cover {
            cmd.arg("-i");
            cmd.arg(cover);
        }
        cmd.args(&["-map", "0:a", "-map", "1:v"]);

        let metadata = [
            format!("title={}", track.title),
            format!("artist={}", track.artist),
            maybe_metadata("album", &self.album_title),
            maybe_metadata("album_artist", &self.album_artist),
            maybe_metadata("date", &self.date),
            format!("disc={}", track.disc),
            format!("track={}", track.track),
        ];
        for m in metadata.iter().filter(|s| !s.is_empty()) {
            cmd.arg("-metadata");
            cmd.arg(m);
        }

        if self.cover.is_some() {
            cmd.args(&[
                "-c:v",
                "copy",
                "-disposition:v",
                "attached_pic",
                "-metadata:s:v",
                "comment=Cover (front)",
            ]);
        }
        cmd.args(&["-c:a", "flac", "-y"]);
        cmd.arg(&output_file);

        let output = cmd.output().map_err(|err| {
            println!("Failed to execute ffmpeg {cmd:?}: {err}");
            output_file.clone()
        })?;

        if output.status.success() {
            println!("OK: {}", output_file.display());
            Ok(())
        } else {
            println!("\nffmpeg FAILED");
            println!("command: {cmd:?}");
            println!("\nstandard output:");
            let _ = stdout().write_all(&output.stdout);
            println!("\nstandard error:");
            let _ = stdout().write_all(&output.stderr);
            Err(output_file)
        }
    }
}

fn run() -> Result<()> {
    let args = Args::parse();
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_path(&args.input_csv)
        .context("failed to open input file")?;

    std::fs::create_dir_all(&args.output_dir).context("failed to create output directory")?;

    for res in reader.deserialize::<Track>() {
        let track = res.context("failed to read CSV")?;
        args.convert_track(&track).map_err(|path| {
            anyhow!(
                "Failed to convert {} into {}",
                track.file.display(),
                path.display()
            )
        })?;
    }
    Ok(())
}

fn main() {
    if let Err(err) = run() {
        println!("Error: {err}");
        std::process::exit(1);
    }
}
