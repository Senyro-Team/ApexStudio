use apex_application::{RobloxGroupSale, RobloxRevenueGroup};
use chrono::{DateTime, NaiveDate, NaiveDateTime, SecondsFormat, Utc};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};

const MAX_FILE_BYTES: u64 = 25 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 100 * 1024 * 1024;
const MAX_ROWS: usize = 100_000;
const MAX_FILES: usize = 20;

#[derive(Debug)]
pub struct CsvPreview {
    pub sales: Vec<RobloxGroupSale>,
    pub skipped: usize,
    pub duplicates: usize,
    pub files: Vec<String>,
    pub warnings: Vec<String>,
    pub from: String,
    pub to: String,
    pub batch_id: String,
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 20
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && value.bytes().any(|byte| byte != b'0')
}

fn amount(value: &str, label: &str) -> Result<u64, String> {
    let value = value.trim();
    let value = value
        .strip_prefix("R$")
        .or_else(|| value.strip_prefix("r$"))
        .unwrap_or(value)
        .trim();
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if (!fraction.is_empty() && !fraction.bytes().all(|byte| byte == b'0'))
        || value.matches('.').count() > 1
    {
        return Err(format!("{label} must be a whole number of Robux."));
    }
    let digits = whole.replace(',', "");
    if digits.is_empty()
        || !digits.bytes().all(|byte| byte.is_ascii_digit())
        || (whole.contains(',')
            && whole.split_once(',').is_none_or(|(first, rest)| {
                first.is_empty()
                    || first.len() > 3
                    || !first.bytes().all(|byte| byte.is_ascii_digit())
                    || rest.split(',').any(|part| {
                        part.len() != 3 || !part.bytes().all(|byte| byte.is_ascii_digit())
                    })
            }))
    {
        return Err(format!("{label} must be a whole number of Robux."));
    }
    let parsed = digits
        .parse::<u64>()
        .map_err(|_| format!("{label} is outside the supported range."))?;
    if parsed > i64::MAX as u64 {
        return Err(format!("{label} is outside the supported range."));
    }
    Ok(parsed)
}

fn sale_date(value: &str) -> Result<String, String> {
    let value = value.trim();
    if let Ok(date) = DateTime::parse_from_rfc3339(value) {
        return Ok(date
            .with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::Millis, true));
    }
    let value = value.strip_suffix(" UTC").unwrap_or(value);
    let formats = [
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M",
        "%m/%d/%Y %I:%M:%S %p",
        "%m/%d/%Y %I:%M %p",
    ];
    for format in formats {
        if let Ok(date) = NaiveDateTime::parse_from_str(value, format) {
            return Ok(date.and_utc().to_rfc3339_opts(SecondsFormat::Millis, true));
        }
    }
    for format in ["%Y-%m-%d", "%m/%d/%Y"] {
        if let Ok(date) = NaiveDate::parse_from_str(value, format) {
            if let Some(date) = date.and_hms_opt(0, 0, 0) {
                return Ok(date.and_utc().to_rfc3339_opts(SecondsFormat::Millis, true));
            }
        }
    }
    Err("A sale row has an invalid UTC date.".into())
}

fn hash(value: &str) -> String {
    Sha256::digest(value.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn row_field<'a>(
    row: &'a csv::StringRecord,
    columns: &HashMap<&str, Option<usize>>,
    key: &str,
) -> &'a str {
    columns
        .get(key)
        .and_then(|index| *index)
        .and_then(|index| row.get(index))
        .unwrap_or_default()
        .trim()
}

pub fn read_csv_files(
    paths: &[impl AsRef<Path>],
    group: &RobloxRevenueGroup,
    existing_sales: &[RobloxGroupSale],
) -> Result<CsvPreview, String> {
    if paths.is_empty() || paths.len() > MAX_FILES {
        return Err(format!("Select between 1 and {MAX_FILES} CSV files."));
    }
    if group.experience_ids.is_empty() {
        return Err("Configure at least one experience ID for this group first.".into());
    }
    let experiences = group
        .experience_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let aliases: [(&str, &[&str]); 9] = [
        ("saleDate", &["saledateandtime", "dateandtime"]),
        ("saleLocation", &["salelocation", "location"]),
        ("universeId", &["universeid"]),
        ("assetId", &["assetid"]),
        ("assetType", &["assettype"]),
        ("holdStatus", &["holdstatus"]),
        ("revenue", &["revenue"]),
        ("price", &["price"]),
        ("transactionId", &["id", "transactionid"]),
    ];
    let required = [
        "saleDate",
        "universeId",
        "assetId",
        "assetType",
        "holdStatus",
        "revenue",
        "price",
    ];
    let mut sales = Vec::new();
    let mut skipped = 0usize;
    let mut warnings = Vec::new();
    let mut files = Vec::new();
    let mut total_bytes = 0u64;
    let mut total_source_rows = 0usize;
    let mut occurrences: HashMap<String, usize> = HashMap::new();

    for path in paths {
        let path = path.as_ref();
        let name = path.file_name().map_or_else(
            || "<unnamed CSV>".to_owned(),
            |value| value.to_string_lossy().into_owned(),
        );
        let metadata = fs::metadata(path).map_err(|error| format!("{name}: {error}"))?;
        if metadata.len() > MAX_FILE_BYTES {
            return Err(format!("{name} is larger than 25 MB."));
        }
        total_bytes = total_bytes.saturating_add(metadata.len());
        if total_bytes > MAX_TOTAL_BYTES {
            return Err("Selected CSV files exceed 100 MB in total.".into());
        }
        let bytes = fs::read(path).map_err(|error| format!("{name}: {error}"))?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(format!(
                "{name} grew beyond the 25 MB limit while being read."
            ));
        }
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| format!("{name} must be a UTF-8 CSV file."))?
            .trim_start_matches('\u{feff}');
        let mut reader = csv::ReaderBuilder::new()
            .flexible(false)
            .from_reader(text.as_bytes());
        let headers = reader
            .headers()
            .map_err(|error| format!("{name}: invalid CSV header: {error}"))?
            .iter()
            .map(normalize)
            .collect::<Vec<_>>();
        let columns = headers
            .iter()
            .enumerate()
            .map(|(index, header)| (header.clone(), index))
            .collect::<HashMap<_, _>>();
        let resolved = aliases
            .iter()
            .map(|(field, options)| {
                (
                    *field,
                    options
                        .iter()
                        .find_map(|alias| columns.get(*alias).copied()),
                )
            })
            .collect::<HashMap<_, _>>();
        let missing = required
            .iter()
            .filter(|field| resolved.get(**field).copied().flatten().is_none())
            .copied()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(format!(
                "{name} is missing required Roblox CSV columns: {}.",
                missing.join(", ")
            ));
        }
        let known = aliases
            .iter()
            .flat_map(|(_, names)| names.iter().copied())
            .chain(["buyeruserid", "universe", "assetname", "locationid"])
            .collect::<HashSet<_>>();
        let unknown = headers
            .iter()
            .filter(|header| !known.contains(header.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        if !unknown.is_empty() {
            warnings.push(format!(
                "{name}: ignored unrecognized column(s): {}.",
                unknown.join(", ")
            ));
        }
        for (line, record) in reader.records().enumerate() {
            total_source_rows += 1;
            if total_source_rows > MAX_ROWS {
                return Err("The selected CSV files contain more than 100,000 rows.".into());
            }
            let record = record.map_err(|error| format!("{name}, row {}: {error}", line + 2))?;
            let asset_type = normalize(row_field(&record, &resolved, "assetType"));
            let universe_id = row_field(&record, &resolved, "universeId");
            if asset_type != "gamepass" || !experiences.contains(universe_id) {
                skipped += 1;
                continue;
            }
            let asset_id = row_field(&record, &resolved, "assetId");
            if !valid_id(universe_id) || !valid_id(asset_id) {
                return Err(format!(
                    "{name}, row {}: invalid experience or Game Pass asset ID.",
                    line + 2
                ));
            }
            let hold_status = match normalize(row_field(&record, &resolved, "holdStatus")).as_str()
            {
                "released" => "Released",
                "held" => "Held",
                "cancelled" | "canceled" => "Cancelled",
                _ => {
                    return Err(format!("{name}, row {}: unknown hold status.", line + 2));
                }
            };
            let raw_sale_date = row_field(&record, &resolved, "saleDate");
            if DateTime::parse_from_rfc3339(raw_sale_date).is_err()
                && !warnings
                    .iter()
                    .any(|warning| warning == "Timezone-free sale times are interpreted as UTC.")
            {
                warnings.push("Timezone-free sale times are interpreted as UTC.".into());
            }
            let sale_at = sale_date(raw_sale_date)
                .map_err(|error| format!("{name}, row {}: {error}", line + 2))?;
            let price = amount(row_field(&record, &resolved, "price"), "Price")
                .map_err(|error| format!("{name}, row {}: {error}", line + 2))?;
            let revenue = amount(row_field(&record, &resolved, "revenue"), "Revenue")
                .map_err(|error| format!("{name}, row {}: {error}", line + 2))?;
            let sale_location = row_field(&record, &resolved, "saleLocation").to_owned();
            let transaction_id = resolved
                .get("transactionId")
                .copied()
                .flatten()
                .and_then(|_| {
                    let value = row_field(&record, &resolved, "transactionId");
                    (!value.is_empty()).then(|| value.to_owned())
                });
            if transaction_id.as_ref().is_some_and(|id| id.len() > 200) {
                return Err(format!(
                    "{name}, row {}: transaction ID is too long.",
                    line + 2
                ));
            }
            let signature = serde_json::to_string(&[
                universe_id,
                asset_id,
                &sale_at,
                &sale_location,
                hold_status,
                &price.to_string(),
                &revenue.to_string(),
            ])
            .map_err(|error| error.to_string())?;
            let fingerprint = if let Some(id) = &transaction_id {
                hash(id)
            } else {
                let occurrence = occurrences.entry(signature.clone()).or_default();
                let fingerprint = hash(&format!("{signature}#{occurrence}"));
                *occurrence += 1;
                fingerprint
            };
            sales.push(RobloxGroupSale {
                group_id: group.group_id.clone(),
                sale_fingerprint: fingerprint,
                universe_id: universe_id.to_owned(),
                asset_id: asset_id.to_owned(),
                sale_at,
                sale_location,
                hold_status: hold_status.into(),
                price,
                revenue,
                import_batch_id: String::new(),
                external_transaction_id: transaction_id.clone(),
                source: "csv".into(),
                dedupe_confidence: if transaction_id.is_some() {
                    "transaction_id".into()
                } else {
                    "exact_fields".into()
                },
                imported_at: None,
            });
        }
        files.push(name);
    }
    if sales.is_empty() {
        return Err(
            "No Game Pass sales for the configured experiences were found in these CSV files."
                .into(),
        );
    }
    let mut seen_fingerprints = existing_sales
        .iter()
        .filter(|sale| sale.group_id == group.group_id)
        .map(|sale| sale.sale_fingerprint.clone())
        .collect::<HashSet<_>>();
    let mut seen_ids = existing_sales
        .iter()
        .filter(|sale| sale.group_id == group.group_id)
        .filter_map(|sale| sale.external_transaction_id.clone())
        .collect::<HashSet<_>>();
    let mut duplicates = 0usize;
    for sale in &sales {
        let duplicate = !seen_fingerprints.insert(sale.sale_fingerprint.clone())
            || sale
                .external_transaction_id
                .as_ref()
                .is_some_and(|id| !seen_ids.insert(id.clone()));
        duplicates += usize::from(duplicate);
    }
    let mut dates = sales
        .iter()
        .map(|sale| sale.sale_at.as_str())
        .collect::<Vec<_>>();
    dates.sort_unstable();
    let from = dates[0][..10].to_owned();
    let to = dates[dates.len() - 1][..10].to_owned();
    let batch_id = format!("cli-csv-{}", Utc::now().timestamp_millis());
    for sale in &mut sales {
        sale.import_batch_id = batch_id.clone();
    }
    if sales
        .iter()
        .all(|sale| sale.external_transaction_id.is_none())
    {
        warnings.push(
            "No transaction IDs were present; duplicate matching uses canonical sale fields."
                .into(),
        );
    }
    Ok(CsvPreview {
        sales,
        skipped,
        duplicates,
        files,
        warnings,
        from,
        to,
        batch_id,
    })
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn group() -> RobloxRevenueGroup {
        RobloxRevenueGroup {
            group_id: "16538500".into(),
            name: "APX".into(),
            experience_ids: vec!["84560840200510".into()],
            pass_mappings: BTreeMap::new(),
            selected_pass_ids: Vec::new(),
            api_updated_at: None,
            api_coverage_from: None,
            api_coverage_to: None,
            api_summary: None,
            api_status: None,
            updated_at: None,
            last_import_at: None,
        }
    }

    #[test]
    fn parses_quoted_roblox_rows_bom_currency_and_ignores_buyer_id() {
        let directory = tempfile::tempdir().expect("temporary CSV directory");
        let path = directory.path().join("sales.csv");
        fs::write(
            &path,
            "\u{feff}Id,Sale Date and Time,Sale Location,Universe Id,Asset Id,Asset Type,Hold Status,Revenue,Price,Buyer User ID\r\n\
             txn-1,11/07/2026 10:31:20 PM,\"Website, Premium\",84560840200510,2053361604,Game Pass,Released,\"R$ 700.0\",\"1,000\",secret-buyer\r\n",
        )
        .expect("write sample CSV");
        let preview = read_csv_files(&[path], &group(), &[]).expect("parse sample CSV");
        assert_eq!(preview.sales.len(), 1);
        assert_eq!(preview.sales[0].price, 1000);
        assert_eq!(preview.sales[0].revenue, 700);
        assert_eq!(preview.sales[0].sale_location, "Website, Premium");
        assert_eq!(preview.sales[0].sale_at, "2026-11-07T22:31:20.000Z");
        assert!(!format!("{preview:?}").contains("secret-buyer"));
    }

    #[test]
    fn malformed_amounts_and_dates_fail_without_importing_partial_rows() {
        let directory = tempfile::tempdir().expect("temporary CSV directory");
        let path = directory.path().join("invalid.csv");
        fs::write(
            &path,
            "Sale Date and Time,Universe Id,Asset Id,Asset Type,Hold Status,Revenue,Price\n2026-11-07,84560840200510,2053361604,Game Pass,Released,not-a-number,1000\n",
        )
        .expect("write invalid CSV");
        assert!(read_csv_files(&[&path], &group(), &[])
            .expect_err("invalid number rejected")
            .contains("Revenue must be a whole number"));
        fs::write(
            &path,
            "Sale Date and Time,Universe Id,Asset Id,Asset Type,Hold Status,Revenue,Price\nnot-a-date,84560840200510,2053361604,Game Pass,Released,700,1000\n",
        )
        .expect("write invalid date CSV");
        assert!(read_csv_files(&[&path], &group(), &[])
            .expect_err("invalid date rejected")
            .contains("invalid UTC date"));
        assert_eq!(amount("R$ 1,234.0", "Price").expect("valid amount"), 1234);
        assert!(amount("12,34", "Price").is_err());
    }
}
