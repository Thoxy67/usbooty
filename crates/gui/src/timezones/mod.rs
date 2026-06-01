//! The canonical Microsoft TimeZone IDs paired with friendly labels.
//!
//! Each entry corresponds to a value Windows accepts in the `<TimeZone>`
//! element of `Microsoft-Windows-Shell-Setup`'s `specialize` pass; the helper
//! writes the ID verbatim into autounattend.xml. Entries are kept in UTC-offset
//! order so the ComboBox built from [`labels`] reads top-to-bottom from
//! Dateline (-12:00) to Kiritimati (+14:00), matching Windows' own picker.

mod data;

use data::TIMEZONES;

/// Newline-separated ComboBox labels, prefixed with their UTC offset; the
/// leading "(Not set)" entry is paired with an empty ID in [`ids`].
pub fn labels() -> String {
    let mut out = String::with_capacity(TIMEZONES.len() * 56);
    out.push_str("(Not set)");
    for tz in TIMEZONES {
        out.push('\n');
        out.push_str(&format!(
            "(UTC{}) {}",
            offset_string(tz.offset_min),
            tz.label
        ));
    }
    out
}

/// Detect the host's IANA time-zone name. Tries `/etc/timezone` first
/// (Debian / Ubuntu / Arch), then the symlink target of `/etc/localtime`
/// (Fedora / openSUSE / macOS layout). Returns `"UTC"` when neither path
/// gives a usable answer.
pub fn host_iana() -> String {
    if let Ok(text) = std::fs::read_to_string("/etc/timezone") {
        let line = text.trim();
        if !line.is_empty() {
            return line.to_string();
        }
    }
    if let Ok(target) = std::fs::read_link("/etc/localtime") {
        // /etc/localtime → /usr/share/zoneinfo/<zone> on Linux
        let s = target.to_string_lossy();
        if let Some(idx) = s.find("zoneinfo/") {
            return s[idx + "zoneinfo/".len()..].to_string();
        }
    }
    String::from("UTC")
}

/// Best-effort host-locale detector. Reads `$LC_ALL`, then `$LANG`, then
/// `$LANGUAGE`, strips the encoding suffix (`en_US.UTF-8` → `en_US`),
/// converts the underscore to a hyphen (BCP-47), and returns `en-US` on
/// the fallback path.
pub fn host_locale() -> String {
    for var in ["LC_ALL", "LANG", "LANGUAGE"] {
        if let Ok(raw) = std::env::var(var) {
            let trimmed = raw.split('.').next().unwrap_or("").trim();
            if !trimmed.is_empty() && trimmed != "C" && trimmed != "POSIX" {
                return trimmed.replacen('_', "-", 1);
            }
        }
    }
    String::from("en-US")
}

/// Newline-separated TimeZone IDs, parallel to [`labels`]; the leading entry
/// is the empty string, matching the "(Not set)" label.
pub fn ids() -> String {
    let mut out = String::with_capacity(TIMEZONES.len() * 32);
    // The leading empty line corresponds to "(Not set)".
    for tz in TIMEZONES {
        out.push('\n');
        out.push_str(tz.id);
    }
    out
}

/// Format an offset in minutes as `±HH:MM`.
fn offset_string(minutes: i32) -> String {
    let sign = if minutes < 0 { '-' } else { '+' };
    let abs = minutes.abs();
    format!("{sign}{:02}:{:02}", abs / 60, abs % 60)
}

/// Convert an IANA zone name (e.g. `Europe/Paris`) to its closest Microsoft
/// TimeZone ID (`Romance Standard Time`). A subset of the CLDR
/// `windowsZones.xml` map, covers the ~60 zones that account for the bulk
/// of real-world hosts. Returns `None` for unknown zones; callers fall back
/// to `UTC` then.
pub fn from_iana(iana: &str) -> Option<&'static str> {
    // Sorted alphabetically by IANA name for ease of editing. Entries follow
    // CLDR's `mapZone` rules: pick the `territory="001"` (default) target for
    // a given IANA zone. A handful of zones with no exact Microsoft match
    // are mapped to their closest neighbour.
    Some(match iana {
        "Africa/Abidjan" | "Africa/Accra" | "Africa/Bamako" | "Africa/Conakry" | "Africa/Dakar"
        | "Africa/Lome" | "Africa/Nouakchott" | "Africa/Ouagadougou" | "Atlantic/Reykjavik" => {
            "Greenwich Standard Time"
        }
        "Africa/Algiers" | "Africa/Tunis" => "W. Central Africa Standard Time",
        "Africa/Cairo" => "Egypt Standard Time",
        "Africa/Casablanca" | "Africa/El_Aaiun" => "Morocco Standard Time",
        "Africa/Johannesburg" | "Africa/Maseru" | "Africa/Mbabane" => "South Africa Standard Time",
        "Africa/Lagos" | "Africa/Bangui" | "Africa/Brazzaville" | "Africa/Douala"
        | "Africa/Kinshasa" | "Africa/Libreville" | "Africa/Luanda" | "Africa/Malabo"
        | "Africa/Niamey" | "Africa/Porto-Novo" => "W. Central Africa Standard Time",
        "Africa/Nairobi"
        | "Africa/Addis_Ababa"
        | "Africa/Asmara"
        | "Africa/Dar_es_Salaam"
        | "Africa/Djibouti"
        | "Africa/Kampala"
        | "Africa/Mogadishu" => "E. Africa Standard Time",
        "America/Anchorage" | "America/Juneau" | "America/Nome" | "America/Sitka"
        | "America/Yakutat" => "Alaskan Standard Time",
        "America/Argentina/Buenos_Aires" | "America/Buenos_Aires" => "Argentina Standard Time",
        "America/Bogota" | "America/Guayaquil" | "America/Lima" | "America/Panama" => {
            "SA Pacific Standard Time"
        }
        "America/Caracas" => "Venezuela Standard Time",
        "America/Chicago"
        | "America/Mexico_City"
        | "America/Monterrey"
        | "America/Tegucigalpa"
        | "America/Winnipeg" => "Central Standard Time",
        "America/Denver" | "America/Edmonton" | "America/Boise" => "Mountain Standard Time",
        "America/Halifax" | "America/Glace_Bay" | "America/Moncton" => "Atlantic Standard Time",
        "America/Havana" => "Cuba Standard Time",
        "America/Indiana/Indianapolis" | "America/Indianapolis" => "US Eastern Standard Time",
        "America/Los_Angeles" | "America/Vancouver" | "America/Tijuana" => "Pacific Standard Time",
        "America/New_York" | "America/Detroit" | "America/Montreal" | "America/Toronto" => {
            "Eastern Standard Time"
        }
        "America/Noronha" => "UTC-02",
        "America/Phoenix" => "US Mountain Standard Time",
        "America/Sao_Paulo" => "E. South America Standard Time",
        "America/St_Johns" => "Newfoundland Standard Time",
        "Antarctica/McMurdo" | "Pacific/Auckland" => "New Zealand Standard Time",
        "Asia/Almaty" => "Central Asia Standard Time",
        "Asia/Baghdad" | "Asia/Aden" | "Asia/Bahrain" | "Asia/Kuwait" | "Asia/Qatar"
        | "Asia/Riyadh" => "Arab Standard Time",
        "Asia/Baku" => "Azerbaijan Standard Time",
        "Asia/Bangkok" | "Asia/Phnom_Penh" | "Asia/Vientiane" | "Asia/Saigon"
        | "Asia/Ho_Chi_Minh" | "Asia/Jakarta" => "SE Asia Standard Time",
        "Asia/Beirut" => "Middle East Standard Time",
        "Asia/Dhaka" => "Bangladesh Standard Time",
        "Asia/Dubai" | "Asia/Muscat" => "Arabian Standard Time",
        "Asia/Hong_Kong" | "Asia/Macau" => "China Standard Time",
        "Asia/Irkutsk" => "North Asia East Standard Time",
        "Asia/Jerusalem" => "Israel Standard Time",
        "Asia/Kabul" => "Afghanistan Standard Time",
        "Asia/Karachi" => "Pakistan Standard Time",
        "Asia/Kathmandu" => "Nepal Standard Time",
        "Asia/Kolkata" | "Asia/Calcutta" | "Asia/Colombo" => "India Standard Time",
        "Asia/Krasnoyarsk" => "North Asia Standard Time",
        "Asia/Manila" => "Singapore Standard Time",
        "Asia/Riyadh87" | "Asia/Riyadh88" | "Asia/Riyadh89" => "Arab Standard Time",
        "Asia/Seoul" | "Asia/Pyongyang" => "Korea Standard Time",
        "Asia/Shanghai" | "Asia/Chongqing" | "Asia/Urumqi" => "China Standard Time",
        "Asia/Singapore" => "Singapore Standard Time",
        "Asia/Taipei" => "Taipei Standard Time",
        "Asia/Tashkent" => "West Asia Standard Time",
        "Asia/Tbilisi" => "Georgian Standard Time",
        "Asia/Tehran" => "Iran Standard Time",
        "Asia/Tokyo" => "Tokyo Standard Time",
        "Asia/Vladivostok" => "Vladivostok Standard Time",
        "Asia/Yakutsk" => "Yakutsk Standard Time",
        "Asia/Yekaterinburg" => "Ekaterinburg Standard Time",
        "Asia/Yerevan" => "Caucasus Standard Time",
        "Atlantic/Azores" => "Azores Standard Time",
        "Atlantic/Cape_Verde" => "Cape Verde Standard Time",
        "Australia/Adelaide" => "Cen. Australia Standard Time",
        "Australia/Brisbane" => "E. Australia Standard Time",
        "Australia/Darwin" => "AUS Central Standard Time",
        "Australia/Hobart" => "Tasmania Standard Time",
        "Australia/Perth" => "W. Australia Standard Time",
        "Australia/Sydney" | "Australia/Melbourne" => "AUS Eastern Standard Time",
        "Europe/Amsterdam" | "Europe/Berlin" | "Europe/Bern" | "Europe/Brussels"
        | "Europe/Copenhagen" | "Europe/Luxembourg" | "Europe/Oslo" | "Europe/Rome"
        | "Europe/Stockholm" | "Europe/Vaduz" | "Europe/Vienna" | "Europe/Zurich" => {
            "W. Europe Standard Time"
        }
        "Europe/Athens" | "Europe/Bucharest" | "Europe/Helsinki" | "Europe/Kiev"
        | "Europe/Kyiv" | "Europe/Mariehamn" | "Europe/Nicosia" | "Europe/Riga"
        | "Europe/Sofia" | "Europe/Tallinn" | "Europe/Vilnius" => "FLE Standard Time",
        "Europe/Belgrade" | "Europe/Bratislava" | "Europe/Budapest" | "Europe/Ljubljana"
        | "Europe/Prague" | "Europe/Warsaw" => "Central European Standard Time",
        "Europe/Istanbul" => "Turkey Standard Time",
        "Europe/Lisbon" => "GMT Standard Time",
        "Europe/London" | "Europe/Dublin" | "Europe/Guernsey" | "Europe/Isle_of_Man"
        | "Europe/Jersey" => "GMT Standard Time",
        "Europe/Madrid" | "Europe/Paris" | "Europe/Andorra" | "Europe/Monaco" | "Africa/Ceuta" => {
            "Romance Standard Time"
        }
        "Europe/Minsk" => "Belarus Standard Time",
        "Europe/Moscow" | "Europe/Volgograd" => "Russian Standard Time",
        "Pacific/Fiji" => "Fiji Standard Time",
        "Pacific/Guam" | "Pacific/Port_Moresby" | "Pacific/Saipan" => "West Pacific Standard Time",
        "Pacific/Honolulu" => "Hawaiian Standard Time",
        "UTC" | "Etc/UTC" | "Etc/GMT" | "Universal" => "UTC",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_and_ids_have_matching_row_counts() {
        assert_eq!(
            labels().split('\n').count(),
            ids().split('\n').count(),
            "the ComboBox label list and the ID list must align"
        );
    }

    #[test]
    fn first_id_is_empty_to_match_not_set_label() {
        let ids = ids();
        let mut rows = ids.split('\n');
        assert_eq!(rows.next(), Some(""));
        assert_eq!(rows.next(), Some("Dateline Standard Time"));
    }

    #[test]
    fn offset_format_is_signed_hh_mm() {
        assert_eq!(offset_string(0), "+00:00");
        assert_eq!(offset_string(60), "+01:00");
        assert_eq!(offset_string(-330), "-05:30");
        assert_eq!(offset_string(345), "+05:45");
    }

    #[test]
    fn romance_standard_time_is_present_with_paris_label() {
        let labels = labels();
        let ids = ids();
        let idx = ids
            .split('\n')
            .position(|i| i == "Romance Standard Time")
            .expect("Romance Standard Time must be in the catalog");
        let label = labels.split('\n').nth(idx).unwrap();
        assert!(label.contains("Paris"), "label was: {label}");
        assert!(label.starts_with("(UTC+01:00)"));
    }
}
