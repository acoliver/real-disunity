//! The reads `--read-target` asks for: the record of a frame whose target is written to a PNG.

use std::path::{Path, PathBuf};

use worldrunner_render_program::unity::offscreen::{PendingImage, TapRead};

use crate::GpuWorkReport;

mod write;

/// One `--read-target FRAME:RECORD:FILE.png`: the frame, the index of the record in the frame's
/// adapted records, and the file the record's colour goes to. The depth goes beside it, to
/// [`depth_file`](Self::depth_file).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetRead {
    pub frame: u64,
    pub record: usize,
    pub file: PathBuf,
}

impl TargetRead {
    /// Reads `FRAME:RECORD:FILE.png`, split at the first two colons so that the file may hold more.
    ///
    /// # Errors
    /// Names what is wrong with `text`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut parts = text.splitn(3, ':');
        let (Some(frame), Some(record), Some(file)) = (parts.next(), parts.next(), parts.next())
        else {
            return Err("is not FRAME:RECORD:FILE.png".to_owned());
        };
        let frame = number(frame, "frame")?;
        let record = number(record, "record")?;
        if file.strip_suffix(".png").is_none_or(str::is_empty) {
            return Err("FILE must end in .png".to_owned());
        }
        Ok(Self {
            frame,
            record,
            file: file.into(),
        })
    }

    /// Refuses a read that repeats the frame and record, or the file, of one in `held`.
    ///
    /// # Errors
    /// Names what is given twice.
    pub fn check_new(&self, held: &[Self]) -> Result<(), String> {
        for other in held {
            if (other.frame, other.record) == (self.frame, self.record) {
                return Err(format!(
                    "frame {} record {} is given twice",
                    self.frame, self.record
                ));
            }
            if other.file == self.file || other.depth_file() == self.file {
                return Err(format!("{} is given twice", self.file.display()));
            }
        }
        Ok(())
    }

    /// Refuses a read of a frame that the run of `frames` frames does not play.
    ///
    /// # Errors
    /// Names the first such read.
    pub fn check_frames(reads: &[Self], frames: u64) -> Result<(), String> {
        match reads.iter().find(|read| read.frame >= frames) {
            Some(read) => Err(format!(
                "--read-target frame {} is not among the {frames} frames the run plays",
                read.frame
            )),
            None => Ok(()),
        }
    }

    /// Where the depth goes: `a.depth.png` for `a.png`.
    #[must_use]
    pub fn depth_file(&self) -> PathBuf {
        let name = self.file.to_string_lossy();
        PathBuf::from(format!(
            "{}.depth.png",
            name.strip_suffix(".png").unwrap_or(&name)
        ))
    }
}

fn number<T: std::str::FromStr>(text: &str, what: &str) -> Result<T, String> {
    text.parse()
        .map_err(|_| format!("{what} `{text}` is not a number"))
}

/// What finishing the reads of a frame left undone.
#[derive(Debug, Default)]
pub(crate) struct Finished {
    /// One line for each colour or depth that was not written, naming the read and the reason.
    pub refusals: Vec<String>,
    /// The reads that wrote no file.
    pub unwritten: usize,
}

impl Finished {
    /// Counts the refusals among the errors of `report`, and the reads left unwritten.
    pub(crate) fn add_to(self, report: &mut GpuWorkReport) {
        for refusal in self.refusals {
            *report.errors.entry(refusal).or_default() += 1;
        }
        report.reads_unwritten += self.unwritten;
    }
}

/// Every read of `reads` refused for `why`, as when the frame never got far enough to read.
pub(crate) fn refuse(reads: &[TargetRead], why: &str) -> Finished {
    Finished {
        refusals: reads
            .iter()
            .map(|read| {
                format!(
                    "read-target frame {} record {}: {why}",
                    read.frame, read.record
                )
            })
            .collect(),
        unwritten: reads.len(),
    }
}

/// Writes the files of the reads of a frame whose submission has finished. `labels` say what each
/// record is, for the line printed for each file. A read that has no tap among `taps` is refused
/// for `missing`.
pub(crate) fn finish(
    reads: &[TargetRead],
    labels: &[(usize, String)],
    mut taps: Vec<TapRead>,
    device: &wgpu::Device,
    missing: &str,
) -> Finished {
    let mut done = Finished::default();
    for read in reads {
        let Some(at) = taps.iter().position(|tap| tap.record == read.record) else {
            done.merge(refuse(std::slice::from_ref(read), missing));
            continue;
        };
        let tap = taps.swap_remove(at);
        let label = labels
            .iter()
            .find(|(record, _)| *record == read.record)
            .map_or("", |(_, label)| label.as_str());
        let named = |what: &str, why: &str| {
            format!(
                "read-target frame {} record {} {what}: {why}",
                read.frame, read.record
            )
        };
        let colour = write_colour(read, label, tap.colour, device);
        let depth = write_depth(read, label, tap.depth, device);
        match (&colour, &depth) {
            (Err(first), Err(second)) if first == second => {
                done.refusals.push(named("colour and depth", first));
            }
            _ => {
                for (what, outcome) in [("colour", &colour), ("depth", &depth)] {
                    match outcome {
                        Ok(line) => eprintln!("{line}"),
                        Err(why) => done.refusals.push(named(what, why)),
                    }
                }
            }
        }
        done.unwritten += usize::from(colour.is_err() && depth.is_err());
    }
    done
}

impl Finished {
    fn merge(&mut self, other: Self) {
        self.refusals.extend(other.refusals);
        self.unwritten += other.unwritten;
    }
}

/// Reads the copy and writes the RGBA PNG of the colour, giving the line that says so.
fn write_colour(
    read: &TargetRead,
    label: &str,
    image: Result<PendingImage, String>,
    device: &wgpu::Device,
) -> Result<String, String> {
    let image = image?;
    let (size, format) = (image.size(), image.format());
    let bytes = image.resolve(device).map_err(|error| error.to_string())?;
    write::write_colour_png(&read.file, &bytes, format, size)?;
    Ok(line(read, (label, "colour"), (size, format), &read.file))
}

/// Reads the copy and writes the 16-bit grey PNG of the depth, giving the line that says so.
fn write_depth(
    read: &TargetRead,
    label: &str,
    image: Result<PendingImage, String>,
    device: &wgpu::Device,
) -> Result<String, String> {
    let image = image?;
    let (size, format) = (image.size(), image.format());
    let bytes = image.resolve(device).map_err(|error| error.to_string())?;
    let grey = write::depth_grey16(&bytes, format, size)?;
    let file = read.depth_file();
    write::write_grey16_png(&file, size, &grey).map_err(|error| error.to_string())?;
    Ok(line(read, (label, "depth"), (size, format), &file))
}

fn line(
    read: &TargetRead,
    (label, what): (&str, &str),
    (size, format): ([u32; 2], wgpu::TextureFormat),
    file: &Path,
) -> String {
    format!(
        "[player] read-target frame {} record {} {label} {what} {}x{} {format:?} -> {}",
        read.frame,
        read.record,
        size[0],
        size[1],
        file.display()
    )
}
