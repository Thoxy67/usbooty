//! The canonical Microsoft TimeZone IDs paired with friendly labels.
//!
//! Each entry corresponds to a value Windows accepts in the `<TimeZone>`
//! element of `Microsoft-Windows-Shell-Setup`'s `specialize` pass; the helper
//! writes the ID verbatim into autounattend.xml. Entries are kept in UTC-offset
//! order so the ComboBox built from [`labels`] reads top-to-bottom from
//! Dateline (-12:00) to Kiritimati (+14:00), matching Windows' own picker.

struct Tz {
    /// Offset from UTC in minutes (negative is west of UTC).
    offset_min: i32,
    /// The Microsoft TimeZone identifier, written to `<TimeZone>` verbatim.
    id: &'static str,
    /// The cities/regions Windows associates with `id`.
    label: &'static str,
}

const TIMEZONES: &[Tz] = &[
    Tz {
        offset_min: -720,
        id: "Dateline Standard Time",
        label: "International Date Line West",
    },
    Tz {
        offset_min: -660,
        id: "UTC-11",
        label: "Coordinated Universal Time-11",
    },
    Tz {
        offset_min: -600,
        id: "Aleutian Standard Time",
        label: "Aleutian Islands",
    },
    Tz {
        offset_min: -600,
        id: "Hawaiian Standard Time",
        label: "Hawaii",
    },
    Tz {
        offset_min: -570,
        id: "Marquesas Standard Time",
        label: "Marquesas Islands",
    },
    Tz {
        offset_min: -540,
        id: "Alaskan Standard Time",
        label: "Alaska",
    },
    Tz {
        offset_min: -540,
        id: "UTC-09",
        label: "Coordinated Universal Time-09",
    },
    Tz {
        offset_min: -480,
        id: "Pacific Standard Time (Mexico)",
        label: "Baja California",
    },
    Tz {
        offset_min: -480,
        id: "UTC-08",
        label: "Coordinated Universal Time-08",
    },
    Tz {
        offset_min: -480,
        id: "Pacific Standard Time",
        label: "Pacific Time (US & Canada)",
    },
    Tz {
        offset_min: -420,
        id: "US Mountain Standard Time",
        label: "Arizona",
    },
    Tz {
        offset_min: -420,
        id: "Mountain Standard Time (Mexico)",
        label: "Chihuahua, La Paz, Mazatlán",
    },
    Tz {
        offset_min: -420,
        id: "Mountain Standard Time",
        label: "Mountain Time (US & Canada)",
    },
    Tz {
        offset_min: -420,
        id: "Yukon Standard Time",
        label: "Yukon",
    },
    Tz {
        offset_min: -360,
        id: "Central America Standard Time",
        label: "Central America",
    },
    Tz {
        offset_min: -360,
        id: "Central Standard Time",
        label: "Central Time (US & Canada)",
    },
    Tz {
        offset_min: -360,
        id: "Easter Island Standard Time",
        label: "Easter Island",
    },
    Tz {
        offset_min: -360,
        id: "Central Standard Time (Mexico)",
        label: "Guadalajara, Mexico City, Monterrey",
    },
    Tz {
        offset_min: -360,
        id: "Canada Central Standard Time",
        label: "Saskatchewan",
    },
    Tz {
        offset_min: -300,
        id: "SA Pacific Standard Time",
        label: "Bogotá, Lima, Quito",
    },
    Tz {
        offset_min: -300,
        id: "Eastern Standard Time (Mexico)",
        label: "Chetumal",
    },
    Tz {
        offset_min: -300,
        id: "Eastern Standard Time",
        label: "Eastern Time (US & Canada)",
    },
    Tz {
        offset_min: -300,
        id: "Haiti Standard Time",
        label: "Haiti",
    },
    Tz {
        offset_min: -300,
        id: "Cuba Standard Time",
        label: "Havana",
    },
    Tz {
        offset_min: -300,
        id: "US Eastern Standard Time",
        label: "Indiana (East)",
    },
    Tz {
        offset_min: -300,
        id: "Turks And Caicos Standard Time",
        label: "Turks and Caicos",
    },
    Tz {
        offset_min: -240,
        id: "Paraguay Standard Time",
        label: "Asunción",
    },
    Tz {
        offset_min: -240,
        id: "Atlantic Standard Time",
        label: "Atlantic Time (Canada)",
    },
    Tz {
        offset_min: -240,
        id: "Venezuela Standard Time",
        label: "Caracas",
    },
    Tz {
        offset_min: -240,
        id: "Central Brazilian Standard Time",
        label: "Cuiabá",
    },
    Tz {
        offset_min: -240,
        id: "SA Western Standard Time",
        label: "Georgetown, La Paz, Manaus, San Juan",
    },
    Tz {
        offset_min: -240,
        id: "Pacific SA Standard Time",
        label: "Santiago",
    },
    Tz {
        offset_min: -210,
        id: "Newfoundland Standard Time",
        label: "Newfoundland",
    },
    Tz {
        offset_min: -180,
        id: "Tocantins Standard Time",
        label: "Araguaina",
    },
    Tz {
        offset_min: -180,
        id: "E. South America Standard Time",
        label: "Brasília",
    },
    Tz {
        offset_min: -180,
        id: "SA Eastern Standard Time",
        label: "Cayenne, Fortaleza",
    },
    Tz {
        offset_min: -180,
        id: "Argentina Standard Time",
        label: "City of Buenos Aires",
    },
    Tz {
        offset_min: -180,
        id: "Greenland Standard Time",
        label: "Greenland",
    },
    Tz {
        offset_min: -180,
        id: "Montevideo Standard Time",
        label: "Montevideo",
    },
    Tz {
        offset_min: -180,
        id: "Magallanes Standard Time",
        label: "Punta Arenas",
    },
    Tz {
        offset_min: -180,
        id: "Saint Pierre Standard Time",
        label: "Saint Pierre and Miquelon",
    },
    Tz {
        offset_min: -180,
        id: "Bahia Standard Time",
        label: "Salvador",
    },
    Tz {
        offset_min: -120,
        id: "UTC-02",
        label: "Coordinated Universal Time-02",
    },
    Tz {
        offset_min: -60,
        id: "Azores Standard Time",
        label: "Azores",
    },
    Tz {
        offset_min: -60,
        id: "Cape Verde Standard Time",
        label: "Cabo Verde Is.",
    },
    Tz {
        offset_min: 0,
        id: "UTC",
        label: "Coordinated Universal Time",
    },
    Tz {
        offset_min: 0,
        id: "Morocco Standard Time",
        label: "Casablanca",
    },
    Tz {
        offset_min: 0,
        id: "GMT Standard Time",
        label: "Dublin, Edinburgh, Lisbon, London",
    },
    Tz {
        offset_min: 0,
        id: "Greenwich Standard Time",
        label: "Monrovia, Reykjavík",
    },
    Tz {
        offset_min: 0,
        id: "Sao Tome Standard Time",
        label: "São Tomé",
    },
    Tz {
        offset_min: 60,
        id: "W. Europe Standard Time",
        label: "Amsterdam, Berlin, Bern, Rome, Stockholm, Vienna",
    },
    Tz {
        offset_min: 60,
        id: "Central Europe Standard Time",
        label: "Belgrade, Bratislava, Budapest, Ljubljana, Prague",
    },
    Tz {
        offset_min: 60,
        id: "Romance Standard Time",
        label: "Brussels, Copenhagen, Madrid, Paris",
    },
    Tz {
        offset_min: 60,
        id: "Central European Standard Time",
        label: "Sarajevo, Skopje, Warsaw, Zagreb",
    },
    Tz {
        offset_min: 60,
        id: "W. Central Africa Standard Time",
        label: "West Central Africa",
    },
    Tz {
        offset_min: 120,
        id: "Jordan Standard Time",
        label: "Amman",
    },
    Tz {
        offset_min: 120,
        id: "GTB Standard Time",
        label: "Athens, Bucharest",
    },
    Tz {
        offset_min: 120,
        id: "Middle East Standard Time",
        label: "Beirut",
    },
    Tz {
        offset_min: 120,
        id: "Egypt Standard Time",
        label: "Cairo",
    },
    Tz {
        offset_min: 120,
        id: "E. Europe Standard Time",
        label: "Chișinău",
    },
    Tz {
        offset_min: 120,
        id: "Syria Standard Time",
        label: "Damascus",
    },
    Tz {
        offset_min: 120,
        id: "West Bank Standard Time",
        label: "Gaza, Hebron",
    },
    Tz {
        offset_min: 120,
        id: "South Africa Standard Time",
        label: "Harare, Pretoria",
    },
    Tz {
        offset_min: 120,
        id: "FLE Standard Time",
        label: "Helsinki, Kyiv, Riga, Sofia, Tallinn, Vilnius",
    },
    Tz {
        offset_min: 120,
        id: "Israel Standard Time",
        label: "Jerusalem",
    },
    Tz {
        offset_min: 120,
        id: "Kaliningrad Standard Time",
        label: "Kaliningrad",
    },
    Tz {
        offset_min: 120,
        id: "Sudan Standard Time",
        label: "Khartoum",
    },
    Tz {
        offset_min: 120,
        id: "Libya Standard Time",
        label: "Tripoli",
    },
    Tz {
        offset_min: 120,
        id: "Namibia Standard Time",
        label: "Windhoek",
    },
    Tz {
        offset_min: 180,
        id: "Arabic Standard Time",
        label: "Baghdad",
    },
    Tz {
        offset_min: 180,
        id: "Turkey Standard Time",
        label: "Istanbul",
    },
    Tz {
        offset_min: 180,
        id: "Arab Standard Time",
        label: "Kuwait, Riyadh",
    },
    Tz {
        offset_min: 180,
        id: "Belarus Standard Time",
        label: "Minsk",
    },
    Tz {
        offset_min: 180,
        id: "Russian Standard Time",
        label: "Moscow, St. Petersburg",
    },
    Tz {
        offset_min: 180,
        id: "E. Africa Standard Time",
        label: "Nairobi",
    },
    Tz {
        offset_min: 210,
        id: "Iran Standard Time",
        label: "Tehran",
    },
    Tz {
        offset_min: 240,
        id: "Arabian Standard Time",
        label: "Abu Dhabi, Muscat",
    },
    Tz {
        offset_min: 240,
        id: "Astrakhan Standard Time",
        label: "Astrakhan, Ulyanovsk",
    },
    Tz {
        offset_min: 240,
        id: "Azerbaijan Standard Time",
        label: "Baku",
    },
    Tz {
        offset_min: 240,
        id: "Russia Time Zone 3",
        label: "Izhevsk, Samara",
    },
    Tz {
        offset_min: 240,
        id: "Mauritius Standard Time",
        label: "Port Louis",
    },
    Tz {
        offset_min: 240,
        id: "Saratov Standard Time",
        label: "Saratov",
    },
    Tz {
        offset_min: 240,
        id: "Georgian Standard Time",
        label: "Tbilisi",
    },
    Tz {
        offset_min: 240,
        id: "Volgograd Standard Time",
        label: "Volgograd",
    },
    Tz {
        offset_min: 240,
        id: "Caucasus Standard Time",
        label: "Yerevan",
    },
    Tz {
        offset_min: 270,
        id: "Afghanistan Standard Time",
        label: "Kabul",
    },
    Tz {
        offset_min: 300,
        id: "West Asia Standard Time",
        label: "Ashgabat, Tashkent",
    },
    Tz {
        offset_min: 300,
        id: "Ekaterinburg Standard Time",
        label: "Ekaterinburg",
    },
    Tz {
        offset_min: 300,
        id: "Pakistan Standard Time",
        label: "Islamabad, Karachi",
    },
    Tz {
        offset_min: 300,
        id: "Qyzylorda Standard Time",
        label: "Qyzylorda",
    },
    Tz {
        offset_min: 330,
        id: "India Standard Time",
        label: "Chennai, Kolkata, Mumbai, New Delhi",
    },
    Tz {
        offset_min: 330,
        id: "Sri Lanka Standard Time",
        label: "Sri Jayawardenepura",
    },
    Tz {
        offset_min: 345,
        id: "Nepal Standard Time",
        label: "Kathmandu",
    },
    Tz {
        offset_min: 360,
        id: "Central Asia Standard Time",
        label: "Astana",
    },
    Tz {
        offset_min: 360,
        id: "Bangladesh Standard Time",
        label: "Dhaka",
    },
    Tz {
        offset_min: 360,
        id: "Omsk Standard Time",
        label: "Omsk",
    },
    Tz {
        offset_min: 390,
        id: "Myanmar Standard Time",
        label: "Yangon (Rangoon)",
    },
    Tz {
        offset_min: 420,
        id: "SE Asia Standard Time",
        label: "Bangkok, Hanoi, Jakarta",
    },
    Tz {
        offset_min: 420,
        id: "Altai Standard Time",
        label: "Barnaul, Gorno-Altaysk",
    },
    Tz {
        offset_min: 420,
        id: "W. Mongolia Standard Time",
        label: "Hovd",
    },
    Tz {
        offset_min: 420,
        id: "North Asia Standard Time",
        label: "Krasnoyarsk",
    },
    Tz {
        offset_min: 420,
        id: "N. Central Asia Standard Time",
        label: "Novosibirsk",
    },
    Tz {
        offset_min: 420,
        id: "Tomsk Standard Time",
        label: "Tomsk",
    },
    Tz {
        offset_min: 480,
        id: "China Standard Time",
        label: "Beijing, Chongqing, Hong Kong",
    },
    Tz {
        offset_min: 480,
        id: "North Asia East Standard Time",
        label: "Irkutsk",
    },
    Tz {
        offset_min: 480,
        id: "Singapore Standard Time",
        label: "Kuala Lumpur, Singapore",
    },
    Tz {
        offset_min: 480,
        id: "W. Australia Standard Time",
        label: "Perth",
    },
    Tz {
        offset_min: 480,
        id: "Taipei Standard Time",
        label: "Taipei",
    },
    Tz {
        offset_min: 480,
        id: "Ulaanbaatar Standard Time",
        label: "Ulaanbaatar",
    },
    Tz {
        offset_min: 525,
        id: "Aus Central W. Standard Time",
        label: "Eucla",
    },
    Tz {
        offset_min: 540,
        id: "Transbaikal Standard Time",
        label: "Chita",
    },
    Tz {
        offset_min: 540,
        id: "Tokyo Standard Time",
        label: "Osaka, Sapporo, Tokyo",
    },
    Tz {
        offset_min: 540,
        id: "North Korea Standard Time",
        label: "Pyongyang",
    },
    Tz {
        offset_min: 540,
        id: "Korea Standard Time",
        label: "Seoul",
    },
    Tz {
        offset_min: 540,
        id: "Yakutsk Standard Time",
        label: "Yakutsk",
    },
    Tz {
        offset_min: 570,
        id: "Cen. Australia Standard Time",
        label: "Adelaide",
    },
    Tz {
        offset_min: 570,
        id: "AUS Central Standard Time",
        label: "Darwin",
    },
    Tz {
        offset_min: 600,
        id: "E. Australia Standard Time",
        label: "Brisbane",
    },
    Tz {
        offset_min: 600,
        id: "AUS Eastern Standard Time",
        label: "Canberra, Melbourne, Sydney",
    },
    Tz {
        offset_min: 600,
        id: "West Pacific Standard Time",
        label: "Guam, Port Moresby",
    },
    Tz {
        offset_min: 600,
        id: "Tasmania Standard Time",
        label: "Hobart",
    },
    Tz {
        offset_min: 600,
        id: "Vladivostok Standard Time",
        label: "Vladivostok",
    },
    Tz {
        offset_min: 630,
        id: "Lord Howe Standard Time",
        label: "Lord Howe Island",
    },
    Tz {
        offset_min: 660,
        id: "Bougainville Standard Time",
        label: "Bougainville Island",
    },
    Tz {
        offset_min: 660,
        id: "Russia Time Zone 10",
        label: "Chokurdakh",
    },
    Tz {
        offset_min: 660,
        id: "Magadan Standard Time",
        label: "Magadan",
    },
    Tz {
        offset_min: 660,
        id: "Norfolk Standard Time",
        label: "Norfolk Island",
    },
    Tz {
        offset_min: 660,
        id: "Sakhalin Standard Time",
        label: "Sakhalin",
    },
    Tz {
        offset_min: 660,
        id: "Central Pacific Standard Time",
        label: "Solomon Is., New Caledonia",
    },
    Tz {
        offset_min: 720,
        id: "Russia Time Zone 11",
        label: "Anadyr, Petropavlovsk-Kamchatsky",
    },
    Tz {
        offset_min: 720,
        id: "New Zealand Standard Time",
        label: "Auckland, Wellington",
    },
    Tz {
        offset_min: 720,
        id: "UTC+12",
        label: "Coordinated Universal Time+12",
    },
    Tz {
        offset_min: 720,
        id: "Fiji Standard Time",
        label: "Fiji",
    },
    Tz {
        offset_min: 765,
        id: "Chatham Islands Standard Time",
        label: "Chatham Islands",
    },
    Tz {
        offset_min: 780,
        id: "UTC+13",
        label: "Coordinated Universal Time+13",
    },
    Tz {
        offset_min: 780,
        id: "Tonga Standard Time",
        label: "Nuku'alofa",
    },
    Tz {
        offset_min: 780,
        id: "Samoa Standard Time",
        label: "Samoa",
    },
    Tz {
        offset_min: 840,
        id: "Line Islands Standard Time",
        label: "Kiritimati Island",
    },
];

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
/// `windowsZones.xml` map — covers the ~60 zones that account for the bulk
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
