use std::str;
use std::vec;
use std::path::{Path, PathBuf};
use std::fs::{read_dir, metadata};
use std::time::SystemTime;
use std::io::Result;

pub struct File {
    pub path: PathBuf,
    pub modified : bool
}

fn visit_dir(dir: &Path, last_updated: SystemTime, result: &mut Vec<File>) -> Result<()> {
    for entry in read_dir(dir)? {
        let entry = entry?;

        let path = entry.path();

        if path.is_dir() {
            visit_dir(&path, last_updated, result);
        } else if let Some(extension) = path.extension() {
            let modified = metadata(&path)?.modified()? > last_updated;
            if extension == "graphql" { result.push(File{path, modified}) }
        }
    }

    Ok(())
}

pub fn find_graphql_files(output: &str, dir: &str) -> Result<(bool, Vec<File>)> {
    let mut result : Vec<File> = vec![];

    let last_updated = std::fs::metadata(output)?.modified()?;

    visit_dir( Path::new(dir), last_updated,  &mut result)?;

    let modified = result.iter().any(|f| f.modified);

    Ok((modified, result))
}