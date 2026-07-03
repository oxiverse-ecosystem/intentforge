// ─── IP-to-Location (MaxMind GeoLite2) ──────────────────────────────
// Loads and queries a GeoLite2-City.mmdb database for IP geolocation.
// The database file is expected at /var/lib/geolite2/GeoLite2-City.mmdb
// and should be updated monthly via the geolite2-update.sh script.
//
// Thread-safe: Reader is Arc-wrapped and Send+Sync.

use maxminddb::{Reader, geoip2};
use std::net::IpAddr;
use std::path::Path;
use std::sync::Arc;

/// Default path for the GeoLite2 database file (Docker/Linux)
/// Falls back to a project-local path for development on Windows.
const DEFAULT_DB_PATH: &str = "/var/lib/geolite2/GeoLite2-City.mmdb";
/// Development fallback path (project-local)
const DEV_DB_PATH: &str = "data/geolite2/GeoLite2-City.mmdb";

/// Geolocation information for an IP address
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct GeoLocation {
    /// ISO country code (e.g., "US", "DE", "IN")
    pub country_code: Option<String>,
    /// Country name
    pub country_name: Option<String>,
    /// Region/state name (e.g., "California", "Bavaria")
    pub region: Option<String>,
    /// City name
    pub city: Option<String>,
    /// Postal code
    pub postal_code: Option<String>,
    /// Latitude
    pub latitude: Option<f64>,
    /// Longitude
    pub longitude: Option<f64>,
    /// Time zone (e.g., "America/New_York")
    pub time_zone: Option<String>,
}

impl GeoLocation {
    /// Derive a BCP-47 language tag from the geolocation (e.g. "en-US", "de-DE").
    /// Used to pass language hints to search engines.
    pub(crate) fn language_tag(&self) -> Option<String> {
        self.country_code.as_ref().map(|cc| {
            let lang = match cc.as_str() {
                "DE" => "de", "AT" => "de", "CH" => "de",
                "FR" => "fr", "BE" => "fr", "CA" => "fr",
                "ES" => "es", "MX" => "es", "AR" => "es",
                "IT" => "it",
                "PT" => "pt", "BR" => "pt",
                "NL" => "nl",
                "RU" => "ru",
                "JP" => "ja",
                "CN" | "TW" => "zh",
                "KR" | "KP" => "ko",
                "SA" | "AE" | "EG" => "ar",
                "TR" => "tr",
                "PL" => "pl",
                "SE" | "NO" | "DK" => "da",
                "FI" => "fi",
                "GR" => "el",
                "CZ" => "cs",
                "HU" => "hu",
                "RO" => "ro",
                "UA" => "uk",
                "IL" => "he",
                "TH" => "th",
                "VN" => "vi",
                "ID" => "id",
                "MY" => "ms",
                "PH" => "tl",
                "IN" | "GB" | "IE" | "AU" | "NZ" | "ZA" | "SG" | "HK" => "en",
                _ => "en",
            };
            format!("{}-{}", lang, cc)
        })
    }
}

/// Thread-safe GeoLite2 reader
pub(crate) struct GeoLocator {
    reader: Arc<Reader<Vec<u8>>>,
    // Note: using open_readfile (Vec<u8> in memory) over open_mmap (unsafe, Mmap)
}

impl GeoLocator {
    /// Load the GeoLite2 database from the default path, with dev fallback.
    /// Returns `None` if the database file doesn't exist or can't be read.
    pub(crate) fn load() -> Option<Self> {
        let prod_path = Path::new(DEFAULT_DB_PATH);
        if prod_path.exists() {
            return Self::load_from(prod_path);
        }
        // Fall back to project-local dev path (Windows/local development)
        let dev_path = Path::new(DEV_DB_PATH);
        if dev_path.exists() {
            tracing::info!(
                "GeoLite2 database not found at default path, using dev fallback: {}",
                DEV_DB_PATH
            );
            return Self::load_from(dev_path);
        }
        tracing::warn!(
            "GeoLite2 database not found at {} or {}. IP geolocation disabled. \
             Run services/geolite2-update.sh to download it.",
            DEFAULT_DB_PATH, DEV_DB_PATH
        );
        None
    }

    /// Load the GeoLite2 database from a specific path.
    pub(crate) fn load_from(path: &Path) -> Option<Self> {
        if !path.exists() {
            tracing::warn!(
                "GeoLite2 database not found at {}. IP geolocation disabled. \
                 Run services/geolite2-update.sh to download it.",
                path.display()
            );
            return None;
        }

        match Reader::open_readfile(path) {
            Ok(reader) => {
                tracing::info!(
                    "GeoLite2 database loaded from {} ({} MB)",
                    path.display(),
                    path.metadata().map(|m| m.len() / 1024 / 1024).unwrap_or(0)
                );
                Some(Self {
                    reader: Arc::new(reader),
                })
            }
            Err(e) => {
                tracing::error!("Failed to load GeoLite2 database: {}", e);
                None
            }
        }
    }

    /// Look up geolocation information for an IP address.
    /// Returns `None` if the IP can't be found in the database.
    pub(crate) fn lookup(&self, ip: IpAddr) -> Option<GeoLocation> {
        match self.reader.lookup(ip) {
            Ok(result) => {
                match result.decode::<geoip2::City>() {
                    Ok(Some(city)) => Some(GeoLocation {
                        country_code: city.country.iso_code.map(|s| s.to_string()),
                        country_name: city.country.names.english.map(|s| s.to_string()),
                        region: city.subdivisions.first()
                            .and_then(|sub| sub.names.english)
                            .map(|s| s.to_string()),
                        city: city.city.names.english.map(|s| s.to_string()),
                        postal_code: city.postal.code.map(|s| s.to_string()),
                        latitude: city.location.latitude,
                        longitude: city.location.longitude,
                        time_zone: city.location.time_zone.map(|s| s.to_string()),
                    }),
                    Ok(None) => None,
                    Err(_) => None,
                }
            }
            Err(e) => {
                tracing::debug!("GeoLite2 lookup failed for {}: {}", ip, e);
                None
            }
        }
    }
}
