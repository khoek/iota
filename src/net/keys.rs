use std::fs;
use std::path::PathBuf;

pub fn read_secret(filename: &str) -> String {
    let path = PathBuf::from(std::env::var("HOME").unwrap())
        .join("secrets")
        .join(filename);
    fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "Couldn't read file: {}",
            path.into_os_string().into_string().unwrap()
        )
    })
}
