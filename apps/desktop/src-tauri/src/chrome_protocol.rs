use serde::{Deserialize, Serialize};
use std::{
    io::{self, Read, Write},
    path::PathBuf,
};
pub const HOST: &str = "com.apexstudio.erlcx";
pub const MAX_FRAME: usize = 1024 * 1024;
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChromeConfig {
    pub chrome_path: String,
    #[serde(default)]
    pub user_data_dir: String,
    pub profile: String,
    pub extension_id: String,
    #[serde(default)]
    pub pipe: String,
}
pub fn directory() -> io::Result<PathBuf> {
    Ok(PathBuf::from(
        std::env::var_os("LOCALAPPDATA")
            .ok_or_else(|| io::Error::other("Local app data is unavailable."))?,
    )
    .join("ApexStudio")
    .join("chrome"))
}
pub fn config() -> io::Result<ChromeConfig> {
    serde_json::from_slice(&std::fs::read(directory()?.join("settings.json"))?)
        .map_err(io::Error::other)
}
pub fn read_frame(reader: &mut impl Read, limit: usize) -> io::Result<Vec<u8>> {
    let mut length = [0; 4];
    reader.read_exact(&mut length)?;
    let size = u32::from_le_bytes(length) as usize;
    if size == 0 || size > limit {
        return Err(io::Error::other("Invalid message size."));
    }
    let mut data = vec![0; size];
    reader.read_exact(&mut data)?;
    Ok(data)
}
pub fn write_frame(writer: &mut impl Write, data: &[u8]) -> io::Result<()> {
    if data.len() > MAX_FRAME {
        return Err(io::Error::other("Response exceeds native messaging limit."));
    }
    writer.write_all(&(data.len() as u32).to_le_bytes())?;
    writer.write_all(data)?;
    writer.flush()
}
pub fn valid_extension(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|c| (b'a'..=b'p').contains(&c))
}
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn framing_rejects_large_and_truncated_frames() {
        assert!(read_frame(&mut &u32::MAX.to_le_bytes()[..], 16).is_err());
        let mut b = vec![];
        write_frame(&mut b, b"hello").unwrap();
        assert_eq!(read_frame(&mut &b[..], 16).unwrap(), b"hello");
        assert!(read_frame(&mut &b[..6], 16).is_err());
    }
    #[test]
    fn extension_ids_are_exact() {
        assert!(valid_extension(&"a".repeat(32)));
        assert!(!valid_extension("anything/else"));
    }
}
