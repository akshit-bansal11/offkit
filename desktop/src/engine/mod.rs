//! Native engines: the work behind the tools, kept free of UI so it is tested without a window.
//! `ffmpeg` is shared by the audio and video families and owned by the lead; every other module
//! belongs to one family, which fills it without editing this file.
//!
//! A family puts logic here once two or more of its tools share it; logic only one tool uses
//! stays in that tool's module (see `tools::base64`). Inside this folder, name the `image`
//! crate `::image`, since `engine::image` shadows it here.

pub mod archive;
pub mod color;
pub mod ffmpeg;
pub mod ffmpeg_audio;
pub mod ffmpeg_video;
pub mod image;
pub mod pdf;
pub mod svg;
pub mod text;
