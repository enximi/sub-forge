use crate::error::AppError;

pub async fn read_limited_response(
    mut response: reqwest::Response,
    max_bytes: usize,
) -> Result<Vec<u8>, AppError> {
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes as u64)
    {
        return Err(AppError::ResponseTooLarge);
    }

    let mut content = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if content.len().saturating_add(chunk.len()) > max_bytes {
            return Err(AppError::ResponseTooLarge);
        }
        content.extend_from_slice(&chunk);
    }
    Ok(content)
}
