use solana_security_txt::security_txt;

#[cfg(not(feature = "no-entrypoint"))]
security_txt! {
    name: "Normal v1",
    project_url: "https://normalfinance.io",
    contacts: "link:https://docs.normalfinance.io/security/bug-bounty",
    policy: "https://github.com/normalfinance/normal-v1/blob/main/SECURITY.md",
    preferred_languages: "en",
    source_code: "https://github.com/normalfinance/normal-v1"
}
