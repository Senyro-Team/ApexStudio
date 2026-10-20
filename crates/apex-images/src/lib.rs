//! Local image intake primitives shared by the desktop app and CLI.
//!
//! No image or filename data leaves this crate. The parser, image format and hash
//! contracts are versioned so callers can persist their results safely.

use png::{BitDepth, ColorType, Decoder, Limits, Transformations};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;
use unicode_normalization::UnicodeNormalization;

pub const PARSER_VERSION: u16 = 1;
pub const PIXEL_HASH_VERSION: u16 = 1;
pub const PHASH_VERSION: u16 = 1;
pub const MAX_FILE_BYTES: usize = 20_000_000;
pub const MAX_DIMENSION: u32 = 8_000;
pub const DEFAULT_INDEX_LIMIT: u32 = 999;
const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
const MAX_DECODER_INTERNAL_BYTES: usize = 32 * 1024 * 1024;
include!("phash_cos_q20.rs");

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Left,
    Right,
    Front,
    Rear,
    Top,
    WindowLeft,
    WindowRight,
    WindowBack,
    WindowTop,
    FrontPlate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    High,
    Inferred,
    Ambiguous,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TextSpan {
    /// Byte offsets into the preserved basename (not the normalized token text).
    pub start: usize,
    pub end: usize,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ParseSuggestion {
    pub original_name: String,
    pub parser_version: u16,
    pub side: Option<Side>,
    pub index: Option<u32>,
    pub index_inferred: bool,
    pub spans: Vec<TextSpan>,
    pub reason: String,
    pub confidence: Confidence,
    pub conflict: bool,
}

#[derive(Debug, Clone)]
struct Token {
    normalized: String,
    start: usize,
    end: usize,
    numeric: bool,
}

#[derive(Debug, Clone, Copy)]
struct Alias {
    words: &'static [&'static str],
    side: Side,
}

const ALIASES: &[Alias] = &[
    Alias {
        words: &["license", "plate", "front"],
        side: Side::FrontPlate,
    },
    Alias {
        words: &["kennzeichen", "vorne"],
        side: Side::FrontPlate,
    },
    Alias {
        words: &["front", "plate"],
        side: Side::FrontPlate,
    },
    Alias {
        words: &["window", "left"],
        side: Side::WindowLeft,
    },
    Alias {
        words: &["left", "window"],
        side: Side::WindowLeft,
    },
    Alias {
        words: &["fenster", "links"],
        side: Side::WindowLeft,
    },
    Alias {
        words: &["window", "right"],
        side: Side::WindowRight,
    },
    Alias {
        words: &["right", "window"],
        side: Side::WindowRight,
    },
    Alias {
        words: &["fenster", "rechts"],
        side: Side::WindowRight,
    },
    Alias {
        words: &["window", "back"],
        side: Side::WindowBack,
    },
    Alias {
        words: &["back", "window"],
        side: Side::WindowBack,
    },
    Alias {
        words: &["window", "rear"],
        side: Side::WindowBack,
    },
    Alias {
        words: &["rear", "window"],
        side: Side::WindowBack,
    },
    Alias {
        words: &["fenster", "hinten"],
        side: Side::WindowBack,
    },
    Alias {
        words: &["window", "top"],
        side: Side::WindowTop,
    },
    Alias {
        words: &["top", "window"],
        side: Side::WindowTop,
    },
    Alias {
        words: &["fenster", "dach"],
        side: Side::WindowTop,
    },
    Alias {
        words: &["left"],
        side: Side::Left,
    },
    Alias {
        words: &["links"],
        side: Side::Left,
    },
    Alias {
        words: &["right"],
        side: Side::Right,
    },
    Alias {
        words: &["rechts"],
        side: Side::Right,
    },
    Alias {
        words: &["front"],
        side: Side::Front,
    },
    Alias {
        words: &["vorne"],
        side: Side::Front,
    },
    Alias {
        words: &["rear"],
        side: Side::Rear,
    },
    Alias {
        words: &["back"],
        side: Side::Rear,
    },
    Alias {
        words: &["hinten"],
        side: Side::Rear,
    },
    Alias {
        words: &["top"],
        side: Side::Top,
    },
    Alias {
        words: &["roof"],
        side: Side::Top,
    },
    Alias {
        words: &["dach"],
        side: Side::Top,
    },
];

/// Suggest a canonical page and index from only the basename, never its parent path.
pub fn parse_filename(filename: &str, index_limit: u32) -> ParseSuggestion {
    let basename = filename.rsplit(['/', '\\']).next().unwrap_or(filename);
    let stem = basename.rsplit_once('.').map_or(basename, |(stem, _)| stem);
    let tokens = tokenize(stem);
    let mut hits: Vec<(Side, usize, usize)> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let best = ALIASES
            .iter()
            .filter(|alias| {
                i + alias.words.len() <= tokens.len()
                    && alias
                        .words
                        .iter()
                        .enumerate()
                        .all(|(offset, word)| tokens[i + offset].normalized == *word)
            })
            .max_by_key(|alias| alias.words.len());
        if let Some(alias) = best {
            hits.push((alias.side, i, alias.words.len()));
            i += alias.words.len();
        } else {
            i += 1;
        }
    }

    let spans: Vec<TextSpan> = hits
        .iter()
        .map(|(_, start, len)| {
            let first = &tokens[*start];
            let last = &tokens[*start + *len - 1];
            TextSpan {
                start: first.start,
                end: last.end,
                text: stem[first.start..last.end].to_owned(),
            }
        })
        .collect();

    if hits.is_empty() {
        return ParseSuggestion {
            original_name: filename.to_owned(),
            parser_version: PARSER_VERSION,
            side: None,
            index: None,
            index_inferred: false,
            spans,
            reason: "Kein bekannter Seitenalias als vollständiges Token gefunden.".into(),
            confidence: Confidence::Ambiguous,
            conflict: true,
        };
    }
    if hits.len() > 1 {
        return ParseSuggestion {
            original_name: filename.to_owned(),
            parser_version: PARSER_VERSION,
            side: None,
            index: None,
            index_inferred: false,
            spans,
            reason: "Mehrere Seitenaliase gefunden; Zuordnung muss manuell bestätigt werden."
                .into(),
            confidence: Confidence::Ambiguous,
            conflict: true,
        };
    }

    let (side, token_start, token_len) = hits[0];
    let alias_end = tokens[token_start + token_len - 1].end;
    let number_token = tokens
        .get(token_start + token_len)
        .filter(|token| token.numeric);
    let parsed = number_token.map(|token| {
        let raw = &stem[token.start..token.end];
        let between = &stem[alias_end..token.start];
        // A single dash is the filename separator (e.g. left-window-2).
        // Doubled dash or whitespace/underscore followed by dash encodes a sign.
        let negative =
            between.ends_with("--") || between.ends_with("_-") || between.ends_with(" -");
        (raw.parse::<u64>(), negative, token)
    });
    if let Some((parsed, negative, token)) = parsed {
        let number = parsed.ok();
        if negative
            || number.is_none()
            || number == Some(0)
            || number.is_some_and(|n| n > index_limit as u64)
            || index_limit == 0
        {
            return ParseSuggestion {
                original_name: filename.to_owned(),
                parser_version: PARSER_VERSION,
                side: Some(side),
                index: None,
                index_inferred: false,
                spans: {
                    let mut value = spans;
                    value.push(TextSpan {
                        start: token.start,
                        end: token.end,
                        text: stem[token.start..token.end].to_owned(),
                    });
                    value
                },
                reason: if negative {
                    "Negativer Seitenindex ist unzulässig.".into()
                } else if number == Some(0) {
                    "Seitenindex 0 ist unzulässig; erwartet wird eine positive Zahl.".into()
                } else {
                    format!("Seitenindex muss zwischen 1 und {index_limit} liegen.")
                },
                confidence: Confidence::Ambiguous,
                conflict: true,
            };
        }
        let mut spans = spans;
        spans.push(TextSpan {
            start: token.start,
            end: token.end,
            text: stem[token.start..token.end].to_owned(),
        });
        return ParseSuggestion {
            original_name: filename.to_owned(), parser_version: PARSER_VERSION,
            side: Some(side), index: number.map(|n| n as u32), index_inferred: false,
            spans, reason: "Genau ein vollständiger Seitenalias und ein gültiger direkt folgender Index gefunden.".into(),
            confidence: Confidence::High, conflict: false,
        };
    }

    ParseSuggestion {
        original_name: filename.to_owned(),
        parser_version: PARSER_VERSION,
        side: Some(side),
        index: Some(1),
        index_inferred: true,
        spans,
        reason: "Genau ein vollständiger Seitenalias gefunden; Index 1 wird sichtbar angenommen."
            .into(),
        confidence: Confidence::Inferred,
        conflict: false,
    }
}

fn tokenize(text: &str) -> Vec<Token> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut tokens = Vec::new();
    let mut start: Option<usize> = None;
    for index in 0..chars.len() {
        let (byte, current) = chars[index];
        let next = chars.get(index + 1).map(|(_, c)| *c);
        let previous = index
            .checked_sub(1)
            .and_then(|i| chars.get(i).map(|(_, c)| *c));
        if !current.is_alphanumeric() {
            if let Some(begin) = start.take() {
                push_token(text, begin, byte, &mut tokens);
            }
            continue;
        }
        let boundary = previous.is_some_and(|prev| {
            let alpha_digit = (prev.is_alphabetic() && current.is_ascii_digit())
                || (prev.is_ascii_digit() && current.is_alphabetic());
            let lower_upper = prev.is_lowercase() && current.is_uppercase();
            let acronym = prev.is_uppercase()
                && current.is_uppercase()
                && next.is_some_and(char::is_lowercase);
            alpha_digit || lower_upper || acronym
        });
        if boundary && let Some(begin) = start.take() {
            push_token(text, begin, byte, &mut tokens);
        }
        start.get_or_insert(byte);
    }
    if let Some(begin) = start {
        push_token(text, begin, text.len(), &mut tokens);
    }
    tokens
}

fn push_token(text: &str, start: usize, end: usize, tokens: &mut Vec<Token>) {
    let original = &text[start..end];
    let normalized: String = original.nfkc().flat_map(char::to_lowercase).collect();
    tokens.push(Token {
        numeric: normalized.chars().all(|c| c.is_ascii_digit()),
        normalized,
        start,
        end,
    });
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateSlot {
    pub variant_key: String,
    pub side: Side,
    pub index: u32,
    pub files: Vec<String>,
}

/// Find slot collisions within each caller-defined design/vehicle variant.
pub fn find_duplicate_slots(items: &[(String, ParseSuggestion)]) -> Vec<DuplicateSlot> {
    let mut grouped: std::collections::BTreeMap<(String, Side, u32), Vec<String>> =
        std::collections::BTreeMap::new();
    for (variant, suggestion) in items {
        if suggestion.conflict {
            continue;
        }
        if let (Some(side), Some(index)) = (suggestion.side, suggestion.index) {
            grouped
                .entry((variant.clone(), side, index))
                .or_default()
                .push(suggestion.original_name.clone());
        }
    }
    grouped
        .into_iter()
        .filter_map(|((variant_key, side, index), files)| {
            (files.len() > 1).then_some(DuplicateSlot {
                variant_key,
                side,
                index,
                files,
            })
        })
        .collect()
}

#[derive(Debug, Error)]
pub enum ImageError {
    #[error("PNG-Datei ist größer als 20.000.000 Bytes ({actual} Bytes).")]
    FileTooLarge { actual: usize },
    #[error("Die ersten 8 Bytes entsprechen keiner PNG-Signatur.")]
    InvalidSignature,
    #[error("PNG kann nicht sicher gelesen oder dekodiert werden: {0}")]
    Decode(String),
    #[error(
        "PNG-Abmessungen sind unzulässig: {width}×{height}; jede Kante muss 1 bis 8000 Pixel haben."
    )]
    InvalidDimensions { width: u32, height: u32 },
    #[error("16-Bit-PNG wird in Phase 4 ausdrücklich nicht unterstützt.")]
    SixteenBit,
    #[error("Animiertes PNG (APNG) wird in Phase 4 ausdrücklich nicht unterstützt.")]
    AnimatedPng,
    #[error("Das dekodierte PNG-Pixelformat wird nicht unterstützt: {0:?}")]
    UnsupportedColorType(ColorType),
    #[error("Dekodierte Bildgröße überschreitet den adressierbaren Speicher.")]
    PixelBufferTooLarge,
    #[error(
        "Bilddatei ist unvollständig oder die dekodierte Abmessung stimmt nicht mit dem Header überein."
    )]
    InconsistentDecode,
    #[error("Eingabepfad ist ein Symlink oder kein regulärer Dateityp.")]
    UnsafeSourcePath,
    #[error("Quellbild änderte sich während des Imports.")]
    SourceChanged,
    #[error("Dateisystemfehler: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ImageMetadata {
    pub width: u32,
    pub height: u32,
    pub byte_len: u64,
    pub file_sha256: String,
    pub pixel_sha256_v1: String,
    pub phash_v1: u64,
}

/// Validate and decode a PNG, then calculate hashes over the original and canonical pixels.
pub fn inspect_png(bytes: &[u8]) -> Result<ImageMetadata, ImageError> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(ImageError::FileTooLarge {
            actual: bytes.len(),
        });
    }
    if bytes.get(..8) != Some(PNG_SIGNATURE) {
        return Err(ImageError::InvalidSignature);
    }
    if bytes.len() < 24 || bytes.get(12..16) != Some(b"IHDR") {
        return Err(ImageError::Decode(
            "PNG ist vor dem vollständigen IHDR-Header abgeschnitten.".into(),
        ));
    }
    let header_width = u32::from_be_bytes(bytes[16..20].try_into().expect("fixed IHDR width"));
    let header_height = u32::from_be_bytes(bytes[20..24].try_into().expect("fixed IHDR height"));
    if header_width == 0
        || header_height == 0
        || header_width > MAX_DIMENSION
        || header_height > MAX_DIMENSION
    {
        return Err(ImageError::InvalidDimensions {
            width: header_width,
            height: header_height,
        });
    }

    let mut decoder = Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(Transformations::EXPAND);
    let limits = Limits {
        bytes: MAX_DECODER_INTERNAL_BYTES,
    };
    decoder.set_limits(limits);
    let mut reader = decoder
        .read_info()
        .map_err(|error| ImageError::Decode(error.to_string()))?;
    let header = reader.info();
    let (width, height) = (header.width, header.height);
    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(ImageError::InvalidDimensions { width, height });
    }
    if header.animation_control.is_some() || header.is_animated() {
        return Err(ImageError::AnimatedPng);
    }
    if header.bit_depth == BitDepth::Sixteen {
        return Err(ImageError::SixteenBit);
    }

    let expected_pixels = (width as usize)
        .checked_mul(height as usize)
        .ok_or(ImageError::PixelBufferTooLarge)?;
    let buffer_size = reader
        .output_buffer_size()
        .ok_or(ImageError::PixelBufferTooLarge)?;
    let mut output = vec![0u8; buffer_size];
    let output_info = reader
        .next_frame(&mut output)
        .map_err(|error| ImageError::Decode(error.to_string()))?;
    if output_info.width != width || output_info.height != height {
        return Err(ImageError::InconsistentDecode);
    }
    let channels = match output_info.color_type {
        ColorType::Grayscale => 1,
        ColorType::GrayscaleAlpha => 2,
        ColorType::Rgb => 3,
        ColorType::Rgba => 4,
        other => return Err(ImageError::UnsupportedColorType(other)),
    };
    if output_info.bit_depth != BitDepth::Eight
        || output_info.buffer_size()
            != expected_pixels
                .checked_mul(channels)
                .ok_or(ImageError::PixelBufferTooLarge)?
    {
        return Err(ImageError::InconsistentDecode);
    }

    let mut pixel_hash = Sha256::new();
    pixel_hash.update(b"APXSTD-PIXEL-v1\0");
    pixel_hash.update(width.to_be_bytes());
    pixel_hash.update(height.to_be_bytes());
    let mut sampled = [[0u8; 32]; 32];
    let sample_x: [usize; 32] =
        std::array::from_fn(|x| (((2 * x + 1) * width as usize) / 64).min(width as usize - 1));
    let sample_y: [usize; 32] =
        std::array::from_fn(|y| (((2 * y + 1) * height as usize) / 64).min(height as usize - 1));
    let mut next_sample_row = 0usize;
    for y in 0..height as usize {
        let row_start = y * width as usize * channels;
        let mut canonical_row = Vec::with_capacity(width as usize * 4);
        for x in 0..width as usize {
            let offset = row_start + x * channels;
            let (mut r, mut g, mut b, alpha) = match output_info.color_type {
                ColorType::Grayscale => (output[offset], output[offset], output[offset], 255),
                ColorType::GrayscaleAlpha => (
                    output[offset],
                    output[offset],
                    output[offset],
                    output[offset + 1],
                ),
                ColorType::Rgb => (output[offset], output[offset + 1], output[offset + 2], 255),
                ColorType::Rgba => (
                    output[offset],
                    output[offset + 1],
                    output[offset + 2],
                    output[offset + 3],
                ),
                other => return Err(ImageError::UnsupportedColorType(other)),
            };
            if alpha == 0 {
                r = 0;
                g = 0;
                b = 0;
            }
            canonical_row.extend_from_slice(&[r, g, b, alpha]);
        }
        pixel_hash.update(&canonical_row);
        while next_sample_row < 32 && sample_y[next_sample_row] == y {
            for (sample_col, &x) in sample_x.iter().enumerate() {
                let offset = x * 4;
                let r = canonical_row[offset] as u32;
                let g = canonical_row[offset + 1] as u32;
                let b = canonical_row[offset + 2] as u32;
                let a = canonical_row[offset + 3] as u32;
                let r = (r * a + 255 * (255 - a) + 127) / 255;
                let g = (g * a + 255 * (255 - a) + 127) / 255;
                let b = (b * a + 255 * (255 - a) + 127) / 255;
                sampled[next_sample_row][sample_col] =
                    ((299 * r + 587 * g + 114 * b + 500) / 1000) as u8;
            }
            next_sample_row += 1;
        }
    }
    if next_sample_row != 32 {
        return Err(ImageError::InconsistentDecode);
    }

    let file_sha256 = hex_lower(&Sha256::digest(bytes));
    let pixel_sha256_v1 = hex_lower(&pixel_hash.finalize());
    let phash_v1 = compute_phash(sampled);
    Ok(ImageMetadata {
        width,
        height,
        byte_len: bytes.len() as u64,
        file_sha256,
        pixel_sha256_v1,
        phash_v1,
    })
}

fn compute_phash(pixels: [[u8; 32]; 32]) -> u64 {
    let mut horizontal = [[0i64; 8]; 32];
    for y in 0..32 {
        for u in 0..8 {
            horizontal[y][u] = (0..32)
                .map(|x| pixels[y][x] as i64 * PHASH_COS_Q20[u][x] as i64)
                .sum();
        }
    }
    let mut coefficients = [0i64; 64];
    for v in 0..8 {
        for u in 0..8 {
            coefficients[v * 8 + u] = (0..32)
                .map(|y| horizontal[y][u] * PHASH_COS_Q20[v][y] as i64)
                .sum();
        }
    }
    let mut non_dc = coefficients[1..].to_vec();
    non_dc.sort_unstable();
    let median = non_dc[non_dc.len() / 2];
    coefficients
        .iter()
        .enumerate()
        .fold(0u64, |hash, (bit, value)| {
            if bit != 0 && *value > median {
                hash | (1u64 << bit)
            } else {
                hash
            }
        })
}

/// Hamming distance for pHash similarity suggestions; it does not establish identity.
pub fn phash_hamming_distance(left: u64, right: u64) -> u32 {
    (left ^ right).count_ones()
}

/// Build the full-SHA suggested display name; provider length limits are checked in Phase 5.
pub fn suggested_roblox_display_name(file_sha256: &str) -> Option<String> {
    (file_sha256.len() == 64
        && file_sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
    .then(|| format!("APXSTD-{file_sha256}"))
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        result.push(HEX[(byte >> 4) as usize] as char);
        result.push(HEX[(byte & 0x0f) as usize] as char);
    }
    result
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Immutable SHA-256-addressed local blob store. Reads revalidate content every time.
#[derive(Debug, Clone)]
pub struct BlobStore {
    root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct BlobRef {
    pub original_name: String,
    pub relative_path: String,
    pub metadata: ImageMetadata,
}

impl BlobStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn import_path(&self, source: &Path) -> Result<BlobRef, ImageError> {
        let before = fs::symlink_metadata(source)?;
        if before.file_type().is_symlink() || !before.is_file() {
            return Err(ImageError::UnsafeSourcePath);
        }
        if before.len() > MAX_FILE_BYTES as u64 {
            return Err(ImageError::FileTooLarge {
                actual: usize::try_from(before.len()).unwrap_or(usize::MAX),
            });
        }
        let mut file = File::open(source)?;
        let opened = file.metadata()?;
        if !opened.is_file() || opened.len() != before.len() {
            return Err(ImageError::SourceChanged);
        }
        let mut bytes = Vec::with_capacity(before.len() as usize);
        Read::by_ref(&mut file)
            .take((MAX_FILE_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_FILE_BYTES {
            return Err(ImageError::FileTooLarge {
                actual: bytes.len(),
            });
        }
        let after = fs::symlink_metadata(source)?;
        if before.len() != after.len()
            || before.modified().ok() != after.modified().ok()
            || bytes.len() as u64 != after.len()
        {
            return Err(ImageError::SourceChanged);
        }
        let original_name = source
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.import_bytes_named(&bytes, original_name)
    }

    pub fn import_bytes(&self, bytes: &[u8]) -> Result<BlobRef, ImageError> {
        self.import_bytes_named(bytes, String::new())
    }

    pub fn import_bytes_named(
        &self,
        bytes: &[u8],
        original_name: String,
    ) -> Result<BlobRef, ImageError> {
        let metadata = inspect_png(bytes)?;
        let relative_path = format!(
            "blobs/{}/{}.png",
            &metadata.file_sha256[..2],
            metadata.file_sha256
        );
        let destination = self.root.join(&relative_path);
        let parent = destination.parent().expect("content path has parent");
        fs::create_dir_all(parent)?;
        let canonical_root = fs::canonicalize(&self.root)?;
        let canonical_parent = fs::canonicalize(parent)?;
        if !canonical_parent.starts_with(&canonical_root) {
            return Err(ImageError::UnsafeSourcePath);
        }
        if fs::symlink_metadata(&destination).is_ok() {
            let entry = fs::symlink_metadata(&destination)?;
            if entry.file_type().is_symlink() || !entry.is_file() {
                return Err(ImageError::UnsafeSourcePath);
            }
            let existing = fs::read(&destination)?;
            if Sha256::digest(&existing).as_slice() != decode_hex(&metadata.file_sha256).as_slice()
            {
                return Err(ImageError::SourceChanged);
            }
            return Ok(BlobRef {
                original_name,
                relative_path,
                metadata,
            });
        }
        let id = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temp = parent.join(format!(
            ".{}.{}.{}.partial",
            metadata.file_sha256,
            std::process::id(),
            id
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        let mut file = options.open(&temp)?;
        if let Err(error) = (|| -> std::io::Result<()> {
            file.write_all(bytes)?;
            file.sync_all()
        })() {
            let _ = fs::remove_file(&temp);
            return Err(error.into());
        }
        drop(file);
        match fs::hard_link(&temp, &destination) {
            Ok(()) => {}
            Err(_error) if fs::symlink_metadata(&destination).is_ok() => {
                let entry = fs::symlink_metadata(&destination)?;
                if entry.file_type().is_symlink() || !entry.is_file() {
                    let _ = fs::remove_file(&temp);
                    return Err(ImageError::UnsafeSourcePath);
                }
                let existing = fs::read(&destination)?;
                if Sha256::digest(&existing).as_slice()
                    != decode_hex(&metadata.file_sha256).as_slice()
                {
                    let _ = fs::remove_file(&temp);
                    return Err(ImageError::SourceChanged);
                }
            }
            Err(error) => {
                let _ = fs::remove_file(&temp);
                return Err(error.into());
            }
        }
        fs::remove_file(temp)?;
        let mut permissions = fs::metadata(&destination)?.permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&destination, permissions)?;
        Ok(BlobRef {
            original_name,
            relative_path,
            metadata,
        })
    }

    pub fn read_verified(&self, blob: &BlobRef) -> Result<Vec<u8>, ImageError> {
        if blob.metadata.file_sha256.len() != 64
            || !blob
                .metadata
                .file_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(ImageError::UnsafeSourcePath);
        }
        let expected_path = format!(
            "blobs/{}/{}.png",
            &blob.metadata.file_sha256[..2],
            blob.metadata.file_sha256
        );
        if blob.relative_path != expected_path {
            return Err(ImageError::UnsafeSourcePath);
        }
        let path = self.root.join(&blob.relative_path);
        let entry = fs::symlink_metadata(&path)?;
        if entry.file_type().is_symlink() || !entry.is_file() {
            return Err(ImageError::UnsafeSourcePath);
        }
        let canonical_root = fs::canonicalize(&self.root)?;
        let canonical_parent =
            fs::canonicalize(path.parent().ok_or(ImageError::UnsafeSourcePath)?)?;
        if !canonical_parent.starts_with(&canonical_root) {
            return Err(ImageError::UnsafeSourcePath);
        }
        if entry.len() > MAX_FILE_BYTES as u64 {
            return Err(ImageError::FileTooLarge {
                actual: usize::try_from(entry.len()).unwrap_or(usize::MAX),
            });
        }
        let bytes = fs::read(path)?;
        let verified = inspect_png(&bytes)?;
        if verified != blob.metadata {
            return Err(ImageError::SourceChanged);
        }
        Ok(bytes)
    }
}

fn decode_hex(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .chunks_exact(2)
        .map(|chunk| {
            let hi = (chunk[0] as char).to_digit(16).unwrap_or(0) as u8;
            let lo = (chunk[1] as char).to_digit(16).unwrap_or(0) as u8;
            hi * 16 + lo
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use png::Encoder;

    fn png_bytes(
        width: u32,
        height: u32,
        color: ColorType,
        depth: BitDepth,
        pixels: &[u8],
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut encoder = Encoder::new(&mut bytes, width, height);
        encoder.set_color(color);
        encoder.set_depth(depth);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(pixels).unwrap();
        drop(writer);
        bytes
    }

    fn set_header_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
        let mut crc = 0xffff_ffffu32;
        for byte in &bytes[12..29] {
            crc ^= *byte as u32;
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xedb8_8320
                } else {
                    crc >> 1
                };
            }
        }
        bytes[29..33].copy_from_slice(&(!crc).to_be_bytes());
    }

    #[test]
    fn filename_reference_cases_and_long_aliases() {
        let cases = [
            (
                "COM-2026-0001_Left1.png",
                Some(Side::Left),
                Some(1),
                Confidence::High,
            ),
            (
                "Police_Right_02.png",
                Some(Side::Right),
                Some(2),
                Confidence::High,
            ),
            (
                "Vehicle_WindowLeft3.png",
                Some(Side::WindowLeft),
                Some(3),
                Confidence::High,
            ),
            (
                "SUV-left-window-2.png",
                Some(Side::WindowLeft),
                Some(2),
                Confidence::High,
            ),
            (
                "Truck_Back1.png",
                Some(Side::Rear),
                Some(1),
                Confidence::High,
            ),
            (
                "Truck_Rear1.png",
                Some(Side::Rear),
                Some(1),
                Confidence::High,
            ),
            (
                "Sedan_FrontPlate1.png",
                Some(Side::FrontPlate),
                Some(1),
                Confidence::High,
            ),
            (
                "LicensePlateFront4.png",
                Some(Side::FrontPlate),
                Some(4),
                Confidence::High,
            ),
            ("Roof4.png", Some(Side::Top), Some(4), Confidence::High),
            (
                "2026_Left.png",
                Some(Side::Left),
                Some(1),
                Confidence::Inferred,
            ),
        ];
        for (name, side, index, confidence) in cases {
            let parsed = parse_filename(name, DEFAULT_INDEX_LIMIT);
            assert_eq!(
                (parsed.side, parsed.index, parsed.confidence),
                (side, index, confidence),
                "{name}"
            );
        }
        assert!(parse_filename("Bright1.png", 999).conflict);
        assert!(parse_filename("Left1_Right2.png", 999).conflict);
        assert!(parse_filename("x/Right2/Left1.png", 999).side == Some(Side::Left));
        assert!(parse_filename("FensterLinks01.png", 999).side == Some(Side::WindowLeft));
        assert!(parse_filename("WindowLeft0.png", 999).conflict);
        assert!(parse_filename("Left--1.png", 999).conflict);
        let aliases = [
            ("Window_Right2.png", Side::WindowRight),
            ("RightWindow2.png", Side::WindowRight),
            ("FensterRechts2.png", Side::WindowRight),
            ("WindowBack2.png", Side::WindowBack),
            ("BackWindow2.png", Side::WindowBack),
            ("FensterHinten2.png", Side::WindowBack),
            ("Window_Top2.png", Side::WindowTop),
            ("TopWindow2.png", Side::WindowTop),
            ("FensterDach2.png", Side::WindowTop),
            ("Front_Plate2.png", Side::FrontPlate),
            ("KennzeichenVorne2.png", Side::FrontPlate),
            ("Links2.png", Side::Left),
            ("Rechts2.png", Side::Right),
            ("Vorne2.png", Side::Front),
            ("Hinten2.png", Side::Rear),
            ("Dach2.png", Side::Top),
        ];
        for (filename, expected_side) in aliases {
            let parsed = parse_filename(filename, 999);
            assert_eq!(parsed.side, Some(expected_side), "{filename}");
            assert_eq!(parsed.index, Some(2), "{filename}");
        }
        assert!(parse_filename("Left10.png", 9).conflict);
    }

    #[test]
    fn detects_duplicates_only_inside_variant() {
        let items = vec![
            ("sedan-v1".into(), parse_filename("A_Left1.png", 9)),
            ("sedan-v1".into(), parse_filename("B_Left01.png", 9)),
            ("suv-v1".into(), parse_filename("C_Left1.png", 9)),
        ];
        let duplicates = find_duplicate_slots(&items);
        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].variant_key, "sedan-v1");
        assert_eq!(duplicates[0].files.len(), 2);
    }

    #[test]
    fn hashes_are_deterministic_and_distinguish_file_from_pixel_content() {
        let one = png_bytes(
            2,
            1,
            ColorType::Rgba,
            BitDepth::Eight,
            &[10, 20, 30, 255, 40, 50, 60, 0],
        );
        let two = png_bytes(
            2,
            1,
            ColorType::Rgba,
            BitDepth::Eight,
            &[10, 20, 30, 255, 1, 2, 3, 0],
        );
        let a = inspect_png(&one).unwrap();
        let b = inspect_png(&two).unwrap();
        assert_eq!(
            a.pixel_sha256_v1,
            "a1cbae2ab2650fb3b043ee38eb9b450f5d1cef7e6ada3990e0b0c9778f6c371a"
        );
        assert_eq!(a.phash_v1, 0x0000_0000_0000_0088);
        assert_ne!(a.file_sha256, b.file_sha256);
        assert_eq!(a.pixel_sha256_v1, b.pixel_sha256_v1);
        assert_eq!(a.phash_v1, inspect_png(&one).unwrap().phash_v1);
        assert_eq!(phash_hamming_distance(a.phash_v1, b.phash_v1), 0);
        assert_eq!(
            suggested_roblox_display_name(&a.file_sha256).unwrap(),
            format!("APXSTD-{}", a.file_sha256)
        );
        assert!(suggested_roblox_display_name("not-a-hash").is_none());
        let different_size = png_bytes(
            1,
            2,
            ColorType::Rgba,
            BitDepth::Eight,
            &[10, 20, 30, 255, 0, 0, 0, 0],
        );
        assert_ne!(
            a.pixel_sha256_v1,
            inspect_png(&different_size).unwrap().pixel_sha256_v1
        );
    }

    #[test]
    fn validates_dimension_edges_and_byte_limit_before_decode() {
        for width in [7_999, 8_000] {
            let pixels = vec![0u8; width as usize * 4];
            let bytes = png_bytes(width, 1, ColorType::Rgba, BitDepth::Eight, &pixels);
            assert_eq!(inspect_png(&bytes).unwrap().width, width);
        }
        let mut over_dimension = png_bytes(1, 1, ColorType::Rgba, BitDepth::Eight, &[0; 4]);
        set_header_u32(&mut over_dimension, 16, 8_001);
        assert!(matches!(
            inspect_png(&over_dimension),
            Err(ImageError::InvalidDimensions { width: 8_001, .. })
        ));
        let mut zero_dimension = png_bytes(1, 1, ColorType::Rgba, BitDepth::Eight, &[0; 4]);
        set_header_u32(&mut zero_dimension, 16, 0);
        assert!(matches!(
            inspect_png(&zero_dimension),
            Err(ImageError::InvalidDimensions { width: 0, .. })
        ));
        let mut gigantic_header = png_bytes(1, 1, ColorType::Rgba, BitDepth::Eight, &[0; 4]);
        set_header_u32(&mut gigantic_header, 16, u32::MAX);
        assert!(matches!(
            inspect_png(&gigantic_header),
            Err(ImageError::InvalidDimensions {
                width: u32::MAX,
                ..
            })
        ));
        let mut over_limit = vec![0u8; MAX_FILE_BYTES + 1];
        over_limit[..8].copy_from_slice(PNG_SIGNATURE);
        assert!(
            matches!(inspect_png(&over_limit), Err(ImageError::FileTooLarge { actual }) if actual == MAX_FILE_BYTES + 1)
        );
        let mut at_limit = vec![0u8; MAX_FILE_BYTES];
        at_limit[..8].copy_from_slice(PNG_SIGNATURE);
        assert!(!matches!(
            inspect_png(&at_limit),
            Err(ImageError::FileTooLarge { .. })
        ));
    }

    #[test]
    fn rejects_invalid_size_depth_and_signature() {
        assert!(matches!(
            inspect_png(b"not png!"),
            Err(ImageError::InvalidSignature)
        ));
        let sixteen = png_bytes(1, 1, ColorType::Rgb, BitDepth::Sixteen, &[0, 1, 0, 2, 0, 3]);
        assert!(matches!(inspect_png(&sixteen), Err(ImageError::SixteenBit)));
        let huge_header = png_bytes(1, 1, ColorType::Rgba, BitDepth::Eight, &[0, 0, 0, 255]);
        let mut forged = huge_header;
        forged[16..20].copy_from_slice(&8001u32.to_be_bytes());
        assert!(matches!(
            inspect_png(&forged),
            Err(ImageError::InvalidDimensions { .. }) | Err(ImageError::Decode(_))
        ));
    }

    #[test]
    fn rejects_animated_png() {
        let mut bytes = Vec::new();
        let mut encoder = Encoder::new(&mut bytes, 1, 1);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(BitDepth::Eight);
        encoder.set_animated(1, 0).unwrap();
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[0, 0, 0, 255]).unwrap();
        drop(writer);
        assert!(matches!(inspect_png(&bytes), Err(ImageError::AnimatedPng)));
    }

    #[test]
    fn stores_frozen_blob_and_verifies_on_read() {
        let dir = std::env::temp_dir().join(format!(
            "apex-images-{}-{}",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let store = BlobStore::new(&dir);
        let bytes = png_bytes(1, 1, ColorType::Rgba, BitDepth::Eight, &[10, 20, 30, 255]);
        let blob = store.import_bytes(&bytes).unwrap();
        assert!(blob.relative_path.starts_with("blobs/"));
        assert_eq!(store.read_verified(&blob).unwrap(), bytes);
        let mut forged = blob.clone();
        forged.relative_path = "../../outside.png".into();
        assert!(matches!(
            store.read_verified(&forged),
            Err(ImageError::UnsafeSourcePath)
        ));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn import_path_keeps_frozen_bytes_after_source_changes() {
        let root = std::env::temp_dir().join(format!(
            "apex-image-source-{}-{}",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("Police_Left1.png");
        let original = png_bytes(1, 1, ColorType::Rgba, BitDepth::Eight, &[1, 2, 3, 255]);
        let changed = png_bytes(1, 1, ColorType::Rgba, BitDepth::Eight, &[4, 5, 6, 255]);
        fs::write(&source, &original).unwrap();
        let store = BlobStore::new(root.join("store"));
        let blob = store.import_path(&source).unwrap();
        assert_eq!(blob.original_name, "Police_Left1.png");
        fs::write(&source, &changed).unwrap();
        assert_eq!(store.read_verified(&blob).unwrap(), original);
        fs::remove_dir_all(root).unwrap();
    }
}
