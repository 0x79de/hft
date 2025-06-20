use anyhow::{Result, anyhow};
use base64::{Engine as _, engine::general_purpose};
use chrono::Utc;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::collections::HashMap;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone)]
pub struct OkxAuth {
    api_key: String,
    secret_key: String,
    passphrase: String,
}

impl OkxAuth {
    pub fn new(api_key: String, secret_key: String, passphrase: String) -> Self {
        Self {
            api_key,
            secret_key,
            passphrase,
        }
    }
    
    pub fn generate_signature(
        &self,
        timestamp: &str,
        method: &str,
        request_path: &str,
        body: &str,
    ) -> Result<String> {
        let message = format!("{}{}{}{}", timestamp, method.to_uppercase(), request_path, body);
        
        let secret_bytes = general_purpose::STANDARD
            .decode(&self.secret_key)
            .map_err(|e| anyhow!("Failed to decode secret key: {}", e))?;
        
        let mut mac = HmacSha256::new_from_slice(&secret_bytes)
            .map_err(|e| anyhow!("Failed to create HMAC: {}", e))?;
        
        mac.update(message.as_bytes());
        let signature = mac.finalize().into_bytes();
        
        Ok(general_purpose::STANDARD.encode(signature))
    }
    
    pub fn get_headers(&self, method: &str, request_path: &str, body: &str) -> Result<HashMap<String, String>> {
        let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        let signature = self.generate_signature(&timestamp, method, request_path, body)?;
        
        let mut headers = HashMap::new();
        headers.insert("OK-ACCESS-KEY".to_string(), self.api_key.clone());
        headers.insert("OK-ACCESS-SIGN".to_string(), signature);
        headers.insert("OK-ACCESS-TIMESTAMP".to_string(), timestamp);
        headers.insert("OK-ACCESS-PASSPHRASE".to_string(), self.passphrase.clone());
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        
        Ok(headers)
    }
    
    pub fn get_websocket_auth(&self) -> Result<WebSocketAuth> {
        let timestamp = Utc::now().timestamp().to_string();
        let message = format!("{}GET/users/self/verify", timestamp);
        
        let secret_bytes = general_purpose::STANDARD
            .decode(&self.secret_key)
            .map_err(|e| anyhow!("Failed to decode secret key: {}", e))?;
        
        let mut mac = HmacSha256::new_from_slice(&secret_bytes)
            .map_err(|e| anyhow!("Failed to create HMAC: {}", e))?;
        
        mac.update(message.as_bytes());
        let signature = mac.finalize().into_bytes();
        let sign = general_purpose::STANDARD.encode(signature);
        
        Ok(WebSocketAuth {
            api_key: self.api_key.clone(),
            passphrase: self.passphrase.clone(),
            timestamp,
            sign,
        })
    }
}

#[derive(Debug, Clone)]
pub struct WebSocketAuth {
    pub api_key: String,
    pub passphrase: String,
    pub timestamp: String,
    pub sign: String,
}

impl WebSocketAuth {
    pub fn to_login_message(&self) -> serde_json::Value {
        serde_json::json!({
            "op": "login",
            "args": [{
                "apiKey": self.api_key,
                "passphrase": self.passphrase,
                "timestamp": self.timestamp,
                "sign": self.sign
            }]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_signature_generation() {
        let auth = OkxAuth::new(
            "test_key".to_string(),
            "dGVzdF9zZWNyZXQ=".to_string(), // base64 encoded "test_secret"
            "test_passphrase".to_string(),
        );
        
        let result = auth.generate_signature(
            "2023-01-01T00:00:00.000Z",
            "GET",
            "/api/v5/account/balance",
            "",
        );
        
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_headers_generation() {
        let auth = OkxAuth::new(
            "test_key".to_string(),
            "dGVzdF9zZWNyZXQ=".to_string(),
            "test_passphrase".to_string(),
        );
        
        let headers = auth.get_headers("GET", "/api/v5/account/balance", "");
        assert!(headers.is_ok());
        
        let headers = headers.unwrap();
        assert!(headers.contains_key("OK-ACCESS-KEY"));
        assert!(headers.contains_key("OK-ACCESS-SIGN"));
        assert!(headers.contains_key("OK-ACCESS-TIMESTAMP"));
        assert!(headers.contains_key("OK-ACCESS-PASSPHRASE"));
    }
}