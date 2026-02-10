use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Country {
    pub country_code: &'static str,
    pub country_en: &'static str,
    pub phone_code: u32,
}

pub const COUNTRIES: &[Country] = &[
    Country {
        country_code: "AL",
        country_en: "Albania",
        phone_code: 355,
    },
    Country {
        country_code: "DZ",
        country_en: "Algeria",
        phone_code: 213,
    },
    Country {
        country_code: "AF",
        country_en: "Afghanistan",
        phone_code: 93,
    },
    Country {
        country_code: "AR",
        country_en: "Argentina",
        phone_code: 54,
    },
    Country {
        country_code: "AE",
        country_en: "United Arab Emirates",
        phone_code: 971,
    },
    Country {
        country_code: "AW",
        country_en: "Aruba",
        phone_code: 297,
    },
    Country {
        country_code: "OM",
        country_en: "Oman",
        phone_code: 968,
    },
    Country {
        country_code: "AZ",
        country_en: "Azerbaijan",
        phone_code: 994,
    },
    Country {
        country_code: "EG",
        country_en: "Egypt",
        phone_code: 20,
    },
    Country {
        country_code: "ET",
        country_en: "Ethiopia",
        phone_code: 251,
    },
    Country {
        country_code: "IE",
        country_en: "Ireland",
        phone_code: 353,
    },
    Country {
        country_code: "EE",
        country_en: "Estonia",
        phone_code: 372,
    },
    Country {
        country_code: "AD",
        country_en: "Andorra",
        phone_code: 376,
    },
    Country {
        country_code: "AO",
        country_en: "Angola",
        phone_code: 244,
    },
    Country {
        country_code: "AI",
        country_en: "Anguilla",
        phone_code: 1264,
    },
    Country {
        country_code: "AG",
        country_en: "Antigua",
        phone_code: 1268,
    },
    Country {
        country_code: "AT",
        country_en: "Austria",
        phone_code: 43,
    },
    Country {
        country_code: "AU",
        country_en: "Australia",
        phone_code: 61,
    },
    Country {
        country_code: "MO",
        country_en: "Macau",
        phone_code: 853,
    },
    Country {
        country_code: "BB",
        country_en: "Barbados",
        phone_code: 1246,
    },
    Country {
        country_code: "PG",
        country_en: "Papua New Guinea",
        phone_code: 675,
    },
    Country {
        country_code: "BS",
        country_en: "The Bahamas",
        phone_code: 1242,
    },
    Country {
        country_code: "PK",
        country_en: "Pakistan",
        phone_code: 92,
    },
    Country {
        country_code: "PY",
        country_en: "Paraguay",
        phone_code: 595,
    },
    Country {
        country_code: "PS",
        country_en: "Palestine",
        phone_code: 970,
    },
    Country {
        country_code: "BH",
        country_en: "Bahrain",
        phone_code: 973,
    },
    Country {
        country_code: "PA",
        country_en: "Panama",
        phone_code: 507,
    },
    Country {
        country_code: "BR",
        country_en: "Brazil",
        phone_code: 55,
    },
    Country {
        country_code: "BY",
        country_en: "Belarus",
        phone_code: 375,
    },
    Country {
        country_code: "BM",
        country_en: "Bermuda",
        phone_code: 1441,
    },
    Country {
        country_code: "BG",
        country_en: "Bulgaria",
        phone_code: 359,
    },
    Country {
        country_code: "MP",
        country_en: "Northern Mariana Islands",
        phone_code: 1670,
    },
    Country {
        country_code: "BJ",
        country_en: "Benin",
        phone_code: 229,
    },
    Country {
        country_code: "BE",
        country_en: "Belgium",
        phone_code: 32,
    },
    Country {
        country_code: "IS",
        country_en: "Iceland",
        phone_code: 354,
    },
    Country {
        country_code: "PR",
        country_en: "Puerto Rico",
        phone_code: 1,
    },
    Country {
        country_code: "PL",
        country_en: "Poland",
        phone_code: 48,
    },
    Country {
        country_code: "BA",
        country_en: "Bosnia and Herzegovina",
        phone_code: 387,
    },
    Country {
        country_code: "BO",
        country_en: "Bolivia",
        phone_code: 591,
    },
    Country {
        country_code: "BZ",
        country_en: "Belize",
        phone_code: 501,
    },
    Country {
        country_code: "PW",
        country_en: "Palau",
        phone_code: 680,
    },
    Country {
        country_code: "BW",
        country_en: "Botswana",
        phone_code: 267,
    },
    Country {
        country_code: "BT",
        country_en: "Bhutan",
        phone_code: 975,
    },
    Country {
        country_code: "BF",
        country_en: "Burkina Faso",
        phone_code: 226,
    },
    Country {
        country_code: "BI",
        country_en: "Burundi",
        phone_code: 257,
    },
    Country {
        country_code: "KP",
        country_en: "North Korea",
        phone_code: 850,
    },
    Country {
        country_code: "GQ",
        country_en: "Equatorial Guinea",
        phone_code: 240,
    },
    Country {
        country_code: "DK",
        country_en: "Denmark",
        phone_code: 45,
    },
    Country {
        country_code: "TL",
        country_en: "Timor-Leste",
        phone_code: 670,
    },
    Country {
        country_code: "TG",
        country_en: "Togo",
        phone_code: 228,
    },
    Country {
        country_code: "DM",
        country_en: "Dominica",
        phone_code: 1767,
    },
    Country {
        country_code: "DO",
        country_en: "Dominican Republic",
        phone_code: 1809,
    },
    Country {
        country_code: "RU",
        country_en: "Russia",
        phone_code: 7,
    },
    Country {
        country_code: "EC",
        country_en: "Ecuador",
        phone_code: 593,
    },
    Country {
        country_code: "ER",
        country_en: "Eritrea",
        phone_code: 291,
    },
    Country {
        country_code: "FO",
        country_en: "Faroe Islands",
        phone_code: 298,
    },
    Country {
        country_code: "PF",
        country_en: "French Polynesia",
        phone_code: 689,
    },
    Country {
        country_code: "GF",
        country_en: "French Guiana",
        phone_code: 594,
    },
    Country {
        country_code: "PM",
        country_en: "Saint Pierre and Miquelon",
        phone_code: 508,
    },
    Country {
        country_code: "VA",
        country_en: "Vatican City",
        phone_code: 39,
    },
    Country {
        country_code: "PH",
        country_en: "Philippines",
        phone_code: 63,
    },
    Country {
        country_code: "FJ",
        country_en: "Fiji",
        phone_code: 679,
    },
    Country {
        country_code: "FI",
        country_en: "Finland",
        phone_code: 358,
    },
    Country {
        country_code: "CV",
        country_en: "Cape Verde",
        phone_code: 238,
    },
    Country {
        country_code: "FK",
        country_en: "Falkland Islands",
        phone_code: 500,
    },
    Country {
        country_code: "GM",
        country_en: "The Gambia",
        phone_code: 220,
    },
    Country {
        country_code: "CG",
        country_en: "Republic of the Congo",
        phone_code: 242,
    },
    Country {
        country_code: "CD",
        country_en: "Democratic Republic of the Congo",
        phone_code: 243,
    },
    Country {
        country_code: "CO",
        country_en: "Colombia",
        phone_code: 57,
    },
    Country {
        country_code: "CR",
        country_en: "Costa Rica",
        phone_code: 506,
    },
    Country {
        country_code: "GD",
        country_en: "Grenada",
        phone_code: 1473,
    },
    Country {
        country_code: "GL",
        country_en: "Greenland",
        phone_code: 299,
    },
    Country {
        country_code: "GE",
        country_en: "Georgia",
        phone_code: 995,
    },
    Country {
        country_code: "GG",
        country_en: "Guernsey",
        phone_code: 44,
    },
    Country {
        country_code: "CU",
        country_en: "Cuba",
        phone_code: 53,
    },
    Country {
        country_code: "GP",
        country_en: "Guadeloupe",
        phone_code: 590,
    },
    Country {
        country_code: "GU",
        country_en: "Guam",
        phone_code: 1671,
    },
    Country {
        country_code: "GY",
        country_en: "Guyana",
        phone_code: 592,
    },
    Country {
        country_code: "KZ",
        country_en: "Kazakhstan",
        phone_code: 7,
    },
    Country {
        country_code: "HT",
        country_en: "Haiti",
        phone_code: 509,
    },
    Country {
        country_code: "KR",
        country_en: "South Korea",
        phone_code: 82,
    },
    Country {
        country_code: "NL",
        country_en: "Netherlands",
        phone_code: 31,
    },
    Country {
        country_code: "BQ",
        country_en: "Bonaire, Sint Eustatius and Saba",
        phone_code: 599,
    },
    Country {
        country_code: "ME",
        country_en: "Montenegro",
        phone_code: 382,
    },
    Country {
        country_code: "HN",
        country_en: "Honduras",
        phone_code: 504,
    },
    Country {
        country_code: "KI",
        country_en: "Kiribati",
        phone_code: 686,
    },
    Country {
        country_code: "DJ",
        country_en: "Djibouti",
        phone_code: 253,
    },
    Country {
        country_code: "KG",
        country_en: "Kyrgyzstan",
        phone_code: 996,
    },
    Country {
        country_code: "GN",
        country_en: "Guinea",
        phone_code: 224,
    },
    Country {
        country_code: "GW",
        country_en: "Guinea-Bissau",
        phone_code: 245,
    },
    Country {
        country_code: "GH",
        country_en: "Ghana",
        phone_code: 233,
    },
    Country {
        country_code: "GA",
        country_en: "Gabon",
        phone_code: 241,
    },
    Country {
        country_code: "KH",
        country_en: "Cambodia",
        phone_code: 855,
    },
    Country {
        country_code: "CZ",
        country_en: "Czech Republic",
        phone_code: 420,
    },
    Country {
        country_code: "ZW",
        country_en: "Zimbabwe",
        phone_code: 263,
    },
    Country {
        country_code: "CM",
        country_en: "Cameroon",
        phone_code: 237,
    },
    Country {
        country_code: "QA",
        country_en: "Qatar",
        phone_code: 974,
    },
    Country {
        country_code: "KY",
        country_en: "Cayman Islands",
        phone_code: 1345,
    },
    Country {
        country_code: "KM",
        country_en: "Comoros",
        phone_code: 269,
    },
    Country {
        country_code: "XK",
        country_en: "Kosovo",
        phone_code: 381,
    },
    Country {
        country_code: "CI",
        country_en: "Côte d'Ivoire",
        phone_code: 225,
    },
    Country {
        country_code: "KW",
        country_en: "Kuwait",
        phone_code: 965,
    },
    Country {
        country_code: "HR",
        country_en: "Croatia",
        phone_code: 385,
    },
    Country {
        country_code: "KE",
        country_en: "Kenya",
        phone_code: 254,
    },
    Country {
        country_code: "CK",
        country_en: "Cook Islands",
        phone_code: 682,
    },
    Country {
        country_code: "CW",
        country_en: "Curaçao",
        phone_code: 599,
    },
    Country {
        country_code: "LV",
        country_en: "Latvia",
        phone_code: 371,
    },
    Country {
        country_code: "LS",
        country_en: "Lesotho",
        phone_code: 266,
    },
    Country {
        country_code: "LA",
        country_en: "Laos",
        phone_code: 856,
    },
    Country {
        country_code: "LB",
        country_en: "Lebanon",
        phone_code: 961,
    },
    Country {
        country_code: "LT",
        country_en: "Lithuania",
        phone_code: 370,
    },
    Country {
        country_code: "LR",
        country_en: "Liberia",
        phone_code: 231,
    },
    Country {
        country_code: "LY",
        country_en: "Libya",
        phone_code: 218,
    },
    Country {
        country_code: "LI",
        country_en: "Liechtenstein",
        phone_code: 423,
    },
    Country {
        country_code: "RE",
        country_en: "Réunion",
        phone_code: 262,
    },
    Country {
        country_code: "LU",
        country_en: "Luxembourg",
        phone_code: 352,
    },
    Country {
        country_code: "RW",
        country_en: "Rwanda",
        phone_code: 250,
    },
    Country {
        country_code: "RO",
        country_en: "Romania",
        phone_code: 40,
    },
    Country {
        country_code: "MG",
        country_en: "Madagascar",
        phone_code: 261,
    },
    Country {
        country_code: "IM",
        country_en: "Isle Of Man",
        phone_code: 44,
    },
    Country {
        country_code: "MV",
        country_en: "Maldives",
        phone_code: 960,
    },
    Country {
        country_code: "MT",
        country_en: "Malta",
        phone_code: 356,
    },
    Country {
        country_code: "MW",
        country_en: "Malawi",
        phone_code: 265,
    },
    Country {
        country_code: "MY",
        country_en: "Malaysia",
        phone_code: 60,
    },
    Country {
        country_code: "ML",
        country_en: "Mali",
        phone_code: 223,
    },
    Country {
        country_code: "MK",
        country_en: "Macedonia",
        phone_code: 389,
    },
    Country {
        country_code: "MH",
        country_en: "Marshall Islands",
        phone_code: 692,
    },
    Country {
        country_code: "MQ",
        country_en: "Martinique",
        phone_code: 596,
    },
    Country {
        country_code: "YT",
        country_en: "Mayotte",
        phone_code: 262,
    },
    Country {
        country_code: "MU",
        country_en: "Mauritius",
        phone_code: 230,
    },
    Country {
        country_code: "MR",
        country_en: "Mauritania",
        phone_code: 222,
    },
    Country {
        country_code: "AS",
        country_en: "American Samoa",
        phone_code: 1684,
    },
    Country {
        country_code: "VI",
        country_en: "US Virgin Islands",
        phone_code: 1340,
    },
    Country {
        country_code: "MN",
        country_en: "Mongolia",
        phone_code: 976,
    },
    Country {
        country_code: "MS",
        country_en: "Montserrat",
        phone_code: 1664,
    },
    Country {
        country_code: "BD",
        country_en: "Bangladesh",
        phone_code: 880,
    },
    Country {
        country_code: "PE",
        country_en: "Peru",
        phone_code: 51,
    },
    Country {
        country_code: "FM",
        country_en: "Federated States of Micronesia",
        phone_code: 691,
    },
    Country {
        country_code: "MM",
        country_en: "Myanmar",
        phone_code: 95,
    },
    Country {
        country_code: "MD",
        country_en: "Moldova",
        phone_code: 373,
    },
    Country {
        country_code: "MA",
        country_en: "Morocco",
        phone_code: 212,
    },
    Country {
        country_code: "MC",
        country_en: "Monaco",
        phone_code: 377,
    },
    Country {
        country_code: "MZ",
        country_en: "Mozambique",
        phone_code: 258,
    },
    Country {
        country_code: "MX",
        country_en: "Mexico",
        phone_code: 52,
    },
    Country {
        country_code: "NA",
        country_en: "Namibia",
        phone_code: 264,
    },
    Country {
        country_code: "ZA",
        country_en: "South Africa",
        phone_code: 27,
    },
    Country {
        country_code: "SS",
        country_en: "South Sudan",
        phone_code: 211,
    },
    Country {
        country_code: "NR",
        country_en: "Nauru",
        phone_code: 674,
    },
    Country {
        country_code: "NI",
        country_en: "Nicaragua",
        phone_code: 505,
    },
    Country {
        country_code: "NP",
        country_en: "Nepal",
        phone_code: 977,
    },
    Country {
        country_code: "NE",
        country_en: "Niger",
        phone_code: 227,
    },
    Country {
        country_code: "NG",
        country_en: "Nigeria",
        phone_code: 234,
    },
    Country {
        country_code: "NU",
        country_en: "Niue",
        phone_code: 683,
    },
    Country {
        country_code: "NO",
        country_en: "Norway",
        phone_code: 47,
    },
    Country {
        country_code: "NF",
        country_en: "Norfolk Island",
        phone_code: 672,
    },
    Country {
        country_code: "PT",
        country_en: "Portugal",
        phone_code: 351,
    },
    Country {
        country_code: "JP",
        country_en: "Japan",
        phone_code: 81,
    },
    Country {
        country_code: "SE",
        country_en: "Sweden",
        phone_code: 46,
    },
    Country {
        country_code: "SV",
        country_en: "El Salvador",
        phone_code: 503,
    },
    Country {
        country_code: "WS",
        country_en: "Samoa",
        phone_code: 685,
    },
    Country {
        country_code: "RS",
        country_en: "Serbia",
        phone_code: 381,
    },
    Country {
        country_code: "SL",
        country_en: "Sierra Leone",
        phone_code: 232,
    },
    Country {
        country_code: "SN",
        country_en: "Senegal",
        phone_code: 221,
    },
    Country {
        country_code: "CY",
        country_en: "Cyprus",
        phone_code: 357,
    },
    Country {
        country_code: "SC",
        country_en: "Seychelles",
        phone_code: 248,
    },
    Country {
        country_code: "SA",
        country_en: "Saudi Arabia",
        phone_code: 966,
    },
    Country {
        country_code: "BL",
        country_en: "Saint Barthélemy",
        phone_code: 590,
    },
    Country {
        country_code: "ST",
        country_en: "Sao Tome and Principe",
        phone_code: 239,
    },
    Country {
        country_code: "SH",
        country_en: "Saint Helena",
        phone_code: 290,
    },
    Country {
        country_code: "KN",
        country_en: "Saint Kitts and Nevis",
        phone_code: 1869,
    },
    Country {
        country_code: "LC",
        country_en: "Saint Lucia",
        phone_code: 1758,
    },
    Country {
        country_code: "MF",
        country_en: "Saint Martin",
        phone_code: 590,
    },
    Country {
        country_code: "SX",
        country_en: "Sint Maarten",
        phone_code: 599,
    },
    Country {
        country_code: "SM",
        country_en: "San Marino",
        phone_code: 378,
    },
    Country {
        country_code: "VC",
        country_en: "Saint Vincent and the Grenadines",
        phone_code: 1784,
    },
    Country {
        country_code: "LK",
        country_en: "Sri Lanka",
        phone_code: 94,
    },
    Country {
        country_code: "SK",
        country_en: "Slovakia",
        phone_code: 421,
    },
    Country {
        country_code: "SI",
        country_en: "Slovenia",
        phone_code: 386,
    },
    Country {
        country_code: "SZ",
        country_en: "Swaziland",
        phone_code: 268,
    },
    Country {
        country_code: "SD",
        country_en: "Sudan",
        phone_code: 249,
    },
    Country {
        country_code: "SR",
        country_en: "Suriname",
        phone_code: 597,
    },
    Country {
        country_code: "SB",
        country_en: "Solomon Islands",
        phone_code: 677,
    },
    Country {
        country_code: "SO",
        country_en: "Somalia",
        phone_code: 252,
    },
    Country {
        country_code: "TJ",
        country_en: "Tajikistan",
        phone_code: 992,
    },
    Country {
        country_code: "TW",
        country_en: "Taiwan",
        phone_code: 886,
    },
    Country {
        country_code: "TH",
        country_en: "Thailand",
        phone_code: 66,
    },
    Country {
        country_code: "TZ",
        country_en: "Tanzania",
        phone_code: 255,
    },
    Country {
        country_code: "TO",
        country_en: "Tonga",
        phone_code: 676,
    },
    Country {
        country_code: "TC",
        country_en: "Turks and Caicos Islands",
        phone_code: 1649,
    },
    Country {
        country_code: "TT",
        country_en: "Trinidad and Tobago",
        phone_code: 1868,
    },
    Country {
        country_code: "TN",
        country_en: "Tunisia",
        phone_code: 216,
    },
    Country {
        country_code: "TV",
        country_en: "Tuvalu",
        phone_code: 688,
    },
    Country {
        country_code: "TR",
        country_en: "Turkey",
        phone_code: 90,
    },
    Country {
        country_code: "TM",
        country_en: "Turkmenistan",
        phone_code: 993,
    },
    Country {
        country_code: "TK",
        country_en: "Tokelau",
        phone_code: 690,
    },
    Country {
        country_code: "WF",
        country_en: "Wallis and Futuna",
        phone_code: 681,
    },
    Country {
        country_code: "VU",
        country_en: "Vanuatu",
        phone_code: 678,
    },
    Country {
        country_code: "GT",
        country_en: "Guatemala",
        phone_code: 502,
    },
    Country {
        country_code: "VE",
        country_en: "Venezuela",
        phone_code: 58,
    },
    Country {
        country_code: "BN",
        country_en: "Brunei",
        phone_code: 673,
    },
    Country {
        country_code: "UG",
        country_en: "Uganda",
        phone_code: 256,
    },
    Country {
        country_code: "UA",
        country_en: "Ukraine",
        phone_code: 380,
    },
    Country {
        country_code: "UY",
        country_en: "Uruguay",
        phone_code: 598,
    },
    Country {
        country_code: "UZ",
        country_en: "Uzbekistan",
        phone_code: 998,
    },
    Country {
        country_code: "GR",
        country_en: "Greece",
        phone_code: 30,
    },
    Country {
        country_code: "ES",
        country_en: "Spain",
        phone_code: 34,
    },
    Country {
        country_code: "EH",
        country_en: "Western Sahara",
        phone_code: 212,
    },
    Country {
        country_code: "HK",
        country_en: "Hong Kong",
        phone_code: 852,
    },
    Country {
        country_code: "SG",
        country_en: "Singapore",
        phone_code: 65,
    },
    Country {
        country_code: "NC",
        country_en: "New Caledonia",
        phone_code: 687,
    },
    Country {
        country_code: "NZ",
        country_en: "New Zealand",
        phone_code: 64,
    },
    Country {
        country_code: "HU",
        country_en: "Hungary",
        phone_code: 36,
    },
    Country {
        country_code: "SY",
        country_en: "Syria",
        phone_code: 963,
    },
    Country {
        country_code: "JM",
        country_en: "Jamaica",
        phone_code: 1876,
    },
    Country {
        country_code: "AM",
        country_en: "Armenia",
        phone_code: 374,
    },
    Country {
        country_code: "YE",
        country_en: "Yemen",
        phone_code: 967,
    },
    Country {
        country_code: "IQ",
        country_en: "Iraq",
        phone_code: 964,
    },
    Country {
        country_code: "IR",
        country_en: "Iran",
        phone_code: 98,
    },
    Country {
        country_code: "IL",
        country_en: "Israel",
        phone_code: 972,
    },
    Country {
        country_code: "IT",
        country_en: "Italy",
        phone_code: 39,
    },
    Country {
        country_code: "IN",
        country_en: "India",
        phone_code: 91,
    },
    Country {
        country_code: "ID",
        country_en: "Indonesia",
        phone_code: 62,
    },
    Country {
        country_code: "VG",
        country_en: "British Virgin Islands",
        phone_code: 1284,
    },
    Country {
        country_code: "IO",
        country_en: "British Indian Ocean Territory",
        phone_code: 246,
    },
    Country {
        country_code: "JO",
        country_en: "Jordan",
        phone_code: 962,
    },
    Country {
        country_code: "VN",
        country_en: "Vietnam",
        phone_code: 84,
    },
    Country {
        country_code: "ZM",
        country_en: "Zambia",
        phone_code: 260,
    },
    Country {
        country_code: "JE",
        country_en: "Jersey",
        phone_code: 44,
    },
    Country {
        country_code: "TD",
        country_en: "Chad",
        phone_code: 235,
    },
    Country {
        country_code: "GI",
        country_en: "Gibraltar",
        phone_code: 350,
    },
    Country {
        country_code: "CL",
        country_en: "Chile",
        phone_code: 56,
    },
    Country {
        country_code: "CF",
        country_en: "Central African Republic",
        phone_code: 236,
    },
    Country {
        country_code: "CN",
        country_en: "China",
        phone_code: 86,
    },
    Country {
        country_code: "US",
        country_en: "United States",
        phone_code: 1,
    },
    Country {
        country_code: "CA",
        country_en: "Canada",
        phone_code: 1,
    },
    Country {
        country_code: "CH",
        country_en: "Switzerland",
        phone_code: 41,
    },
    Country {
        country_code: "GB",
        country_en: "United Kingdom",
        phone_code: 44,
    },
    Country {
        country_code: "FR",
        country_en: "France",
        phone_code: 33,
    },
    Country {
        country_code: "DE",
        country_en: "Germany",
        phone_code: 49,
    },
];
