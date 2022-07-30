use std::borrow::Cow;
use std::fmt::Display;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use deunicode::deunicode;
use rayon::prelude::*;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Track {
    file: PathBuf,
    disc: Option<u32>,
    track: Option<u32>,
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
    #[clap(short = 'j', long)]
    threads: Option<usize>,
    #[clap(short, long)]
    verbose: bool,

    /// CSV file containing columns: file, disc, track, title, artist
    input_csv: PathBuf,

    /// Directory to write output files
    output_dir: PathBuf,
}

fn maybe_metadata<T: Display>(key: &str, val: &Option<T>) -> String {
    match val {
        Some(ref val) => format!("{key}={val}"),
        None => String::new(),
    }
}

impl Args {
    fn convert_track(&self, track: &Track) -> Result<()> {
        let input_file = match &self.input_dir {
            Some(dir) => Cow::Owned(dir.join(&track.file)),
            None => Cow::Borrowed(&track.file),
        };

        let prefix = match (track.disc, track.track) {
            (Some(disc), Some(track)) => format!("{disc}.{track:02}-"),
            (Some(disc), None) => format!("{disc}-"),
            (None, Some(track)) => format!("{track:02}-"),
            (None, None) => String::new(),
        };
        let output_file = self.output_dir.join(format!(
            "{prefix}{artist}-{title}.flac",
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
            maybe_metadata("disc", &track.disc),
            maybe_metadata("track", &track.track),
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

        if self.verbose {
            println!("+ {cmd:?}");
        }

        let output = cmd
            .output()
            .with_context(|| "Failed to execute ffmpeg {cmd:?}")?;
        if output.status.success() {
            println!("OK: {}", output_file.display());
            Ok(())
        } else {
            Err(anyhow!(
                "failed to convert {infile} into {outfile}: ffmpeg command failed\n\
                 \n\
                 command: {cmd:?}\n\
                 \n\
                 standard output:\n\
                 {stdout}\n\
                 \n\
                 standard error:\n\
                 {stderr}\n",
                infile = track.file.display(),
                outfile = output_file.display(),
                cmd = cmd,
                stdout = String::from_utf8_lossy(&output.stdout),
                stderr = String::from_utf8_lossy(&output.stderr),
            ))
        }
    }
}

fn run() -> Result<()> {
    let args = Args::parse();

    if let Some(threads) = args.threads {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build_global()
            .context("failed to initialize rayon global thread pool")?;
    }

    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_path(&args.input_csv)
        .context("failed to open input file")?;

    std::fs::create_dir_all(&args.output_dir).context("failed to create output directory")?;

    // Neat, you can collect from an iterator of Results into a Result of a collection. Returns
    // Ok(collection) if every value was Ok, or Err(e) of the first Err item.
    let tracks = reader
        .deserialize()
        .collect::<Result<Vec<Track>, _>>()
        .context("failed to parse CSV file")?;

    // short-circuits returning the first error, or Ok(()) on success
    tracks
        .par_iter()
        .try_for_each(|track| args.convert_track(track))
}

fn main() {
    if let Err(err) = run() {
        println!("Error: {err:#}");
        std::process::exit(1);
    }
}
