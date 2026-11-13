use edtech_avatar_pipeline::{deadline_status, InfraiClient};
use std::{env, fs};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let user_id = env::var("LEARNER_ID")?;
    let filename = env::var("AVATAR_FILE")?;
    let deadline = env::var("COURSE_DEADLINE")?;
    let now = env::var("CURRENT_TIME")?;
    deadline_status(&deadline, &now)?;
    let client = InfraiClient::from_environment()?;
    let result = client.process_avatar(&user_id, &filename, fs::read(&filename)?).await?;
    println!("avatar updated for learner {user_id}: {result}");
    Ok(())
}
