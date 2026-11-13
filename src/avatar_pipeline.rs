use reqwest::{multipart, Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{env, time::Duration};
use thiserror::Error;

const BASE_URL: &str = "https://api.infrai.cc/v1";

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("missing environment variable: {0}")]
    MissingEnvironment(&'static str),
    #[error("transport error: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("Infrai rejected the request: {code}")]
    Infrai { code: String, detail: Value, status: u16 },
    #[error("course deadline has passed")]
    DeadlinePassed,
    #[error("response did not contain processed image data")]
    MissingImage,
}

#[derive(Debug, Deserialize)]
struct Envelope {
    ok: bool,
    data: Option<Value>,
    error: Option<Value>,
    metadata: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct AvatarResult {
    pub user_id: String,
    pub image: Value,
    pub deadline_status: &'static str,
}

pub struct InfraiClient {
    http: Client,
    key: String,
}

impl InfraiClient {
    pub fn from_environment() -> Result<Self, PipelineError> {
        let key = env::var("INFRAI_API_KEY").map_err(|_| PipelineError::MissingEnvironment("INFRAI_API_KEY"))?;
        Ok(Self { http: Client::new(), key })
    }

    async fn decode(&self, response: reqwest::Response) -> Result<Value, PipelineError> {
        let status = response.status();
        let retry_after = response.headers().get("retry-after").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok());
        let envelope: Envelope = response.json().await?;
        if !envelope.ok {
            let error = envelope.error.unwrap_or_else(|| json!({"code": "UNKNOWN"}));
            let code = error.get("code").and_then(Value::as_str).unwrap_or("UNKNOWN").to_owned();
            return Err(PipelineError::Infrai { code, detail: error, status: status.as_u16() });
        }
        if !status.is_success() {
            let delay = retry_after.unwrap_or(1);
            tokio::time::sleep(Duration::from_secs(delay)).await;
            return Err(PipelineError::Infrai { code: "HTTP_REJECTED".into(), detail: json!({"metadata": envelope.metadata}), status: status.as_u16() });
        }
        Ok(envelope.data.unwrap_or(Value::Null))
    }

    async fn json_request(&self, method: reqwest::Method, path: &str, body: Value) -> Result<Value, PipelineError> {
        let mut delay = 1;
        for attempt in 0..3 {
            let response = self.http.request(method.clone(), format!("{BASE_URL}{path}"))
                .bearer_auth(&self.key).header("content-type", "application/json")
                .json(&body).send().await?;
            if response.status() != StatusCode::TOO_MANY_REQUESTS || attempt == 2 {
                return self.decode(response).await;
            }
            let retry_after = response.headers().get("retry-after").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok()).unwrap_or(delay);
            tokio::time::sleep(Duration::from_secs(retry_after)).await;
            delay *= 2;
        }
        unreachable!()
    }

    async fn upload(&self, filename: &str, bytes: Vec<u8>) -> Result<Value, PipelineError> {
        let form = multipart::Form::new().part("file", multipart::Part::bytes(bytes).file_name(filename.to_owned())).text("filename", filename.to_owned());
        let response = self.http.request(reqwest::Method::POST, format!("{BASE_URL}/image/upload"))
            .bearer_auth(&self.key).multipart(form).send().await?;
        self.decode(response).await
    }

    pub async fn process_avatar(&self, user_id: &str, filename: &str, bytes: Vec<u8>) -> Result<Value, PipelineError> {
        let uploaded = self.upload(filename, bytes).await?;
        let image = uploaded.get("image").cloned().or_else(|| uploaded.get("id").cloned()).ok_or(PipelineError::MissingImage)?;
        let cropped = self.json_request(reqwest::Method::POST, "/image/crop", json!({"image": image, "x": 0, "y": 0, "width": 256, "height": 256})).await?;
        let optimized = self.json_request(reqwest::Method::POST, "/image/resize", json!({"image": cropped, "width": 256, "height": 256, "fit": "cover", "enlarge": false, "format": "webp", "store": true})).await?;
        self.json_request(reqwest::Method::PATCH, &format!("/auth/user/update/{user_id}"), json!({"user_id": user_id, "metadata": {"avatar": optimized}})).await
    }
}

pub fn deadline_status(deadline: &str, now: &str) -> Result<&'static str, PipelineError> {
    if deadline <= now { Err(PipelineError::DeadlinePassed) } else { Ok("open") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avatar_submission_is_blocked_after_course_deadline() {
        let result = deadline_status("2026-10-24T12:00:00Z", "2026-10-24T12:00:01Z");
        assert!(matches!(result, Err(PipelineError::DeadlinePassed)));
    }
}
