use serde::{Deserialize, Serialize};

use crate::error::AppError;

pub const MAX_SCRIPTS_BYTES: usize = 64 * 1024;

// 用户代码随编码链接传递，服务端不保存脚本记录。
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserScripts {
    #[serde(default)]
    pub subscription: String,
    #[serde(default)]
    pub preprocess_rules: String,
    #[serde(default)]
    pub postprocess_config: String,
}

impl UserScripts {
    pub fn validate(&self) -> Result<(), AppError> {
        let total =
            self.subscription.len() + self.preprocess_rules.len() + self.postprocess_config.len();
        if total > MAX_SCRIPTS_BYTES {
            return Err(AppError::BadRequest("脚本总长度不能超过 64 KiB".into()));
        }
        Ok(())
    }
}
