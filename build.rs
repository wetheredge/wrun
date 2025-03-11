use std::path::PathBuf;
use std::{env, fs, io};

fn main() -> io::Result<()> {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let target = env::var("TARGET").unwrap();
    fs::write(out_dir.join("target"), target)
}
