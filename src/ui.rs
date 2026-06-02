use axum::http::{HeaderMap, header};

pub(crate) fn wants_html(headers: &HeaderMap) -> bool {
    let accept = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !accept
        .split(',')
        .any(|part| part.trim().starts_with("text/html"))
    {
        return false;
    }
    let ua = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    ![
        "curl",
        "wget",
        "aws-cli",
        "boto3",
        "restic",
        "rclone",
        "go-http-client",
        "python-requests",
    ]
    .iter()
    .any(|tool| ua.contains(tool))
}

pub(crate) fn file_browser_html(config_path: &str) -> String {
    let config_path_json =
        serde_json::to_string(config_path).unwrap_or_else(|_| "\"/api/config.yaml\"".to_string());
    include_str!("ui.html").replace("__CONFIG_PATH_JSON__", &config_path_json)
}
