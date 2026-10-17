use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use std::io::{Read, Write};

use crate::{error::AppError, policy::Policy, user_scripts::UserScripts};

mod compact;

const TOKEN_PREFIX: &str = "v1.";
const MAX_TICKET_BYTES: usize = 128 * 1024;
const MAX_TOKEN_CHARS: usize = 8192;

#[derive(Clone, Copy, PartialEq)]
pub enum Scope {
    Feed,
    Subscription,
    Rules,
    RuleResource,
}

pub struct Ticket {
    pub scope: Scope,
    pub source_url: Option<String>,
    pub converter_url: Option<String>,
    pub scripts: Option<UserScripts>,
    pub policy: Option<Policy>,
    pub script: Option<String>,
}

pub fn encode(ticket: &Ticket) -> Result<String, AppError> {
    let compact = compact::CompactTicket::try_from(ticket)?;
    let content = serde_json::to_vec(&compact).map_err(|_| AppError::Internal)?;
    if content.len() > MAX_TICKET_BYTES {
        return Err(AppError::BadRequest("脚本与链接内容过长".into()));
    }
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(&content)
        .map_err(|_| AppError::Internal)?;
    let compressed = encoder.finish().map_err(|_| AppError::Internal)?;
    // 配置直接压缩编码进链接，版本前缀用于识别格式，不提供加密或签名。
    let token = format!("{TOKEN_PREFIX}{}", URL_SAFE_NO_PAD.encode(compressed));
    if token.len() > MAX_TOKEN_CHARS {
        return Err(AppError::BadRequest(
            "脚本与链接压缩后仍过长，请缩短后重新生成".into(),
        ));
    }
    Ok(token)
}

pub fn decode(token: &str, scope: Scope) -> Result<Ticket, AppError> {
    if token.len() > MAX_TOKEN_CHARS {
        return Err(AppError::Unauthorized);
    }
    let encoded = token
        .strip_prefix(TOKEN_PREFIX)
        .ok_or(AppError::Unauthorized)?;
    let compressed = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| AppError::Unauthorized)?;
    // 链接内容由调用方提供，限制解压输出，避免压缩数据耗尽内存。
    let mut content = Vec::new();
    ZlibDecoder::new(compressed.as_slice())
        .take((MAX_TICKET_BYTES + 1) as u64)
        .read_to_end(&mut content)
        .map_err(|_| AppError::Unauthorized)?;
    if content.len() > MAX_TICKET_BYTES {
        return Err(AppError::Unauthorized);
    }
    let ticket: compact::CompactTicket =
        serde_json::from_slice(&content).map_err(|_| AppError::Unauthorized)?;
    let ticket = Ticket::from(ticket);
    if ticket.scope != scope {
        return Err(AppError::Unauthorized);
    }
    if let Some(scripts) = &ticket.scripts {
        scripts.validate()?;
    }
    if ticket
        .script
        .as_ref()
        .is_some_and(|script| script.len() > crate::user_scripts::MAX_SCRIPTS_BYTES)
    {
        return Err(AppError::BadRequest("脚本总长度不能超过 64 KiB".into()));
    }
    Ok(ticket)
}
