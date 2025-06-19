use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OkxTicker {
    #[serde(rename = "instId")]
    pub inst_id: String,
    #[serde(rename = "last")]
    pub last: String,
    #[serde(rename = "lastSz")]
    pub last_sz: String,
    #[serde(rename = "askPx")]
    pub ask_px: String,
    #[serde(rename = "askSz")]
    pub ask_sz: String,
    #[serde(rename = "bidPx")]
    pub bid_px: String,
    #[serde(rename = "bidSz")]
    pub bid_sz: String,
    #[serde(rename = "open24h")]
    pub open_24h: String,
    #[serde(rename = "high24h")]
    pub high_24h: String,
    #[serde(rename = "low24h")]
    pub low_24h: String,
    #[serde(rename = "vol24h")]
    pub vol_24h: String,
    #[serde(rename = "volCcy24h")]
    pub vol_ccy_24h: String,
    #[serde(rename = "ts")]
    pub ts: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OkxOrderBook {
    #[serde(rename = "asks")]
    pub asks: Vec<Vec<String>>,
    #[serde(rename = "bids")]
    pub bids: Vec<Vec<String>>,
    #[serde(rename = "ts")]
    pub ts: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OkxAccountBalance {
    #[serde(rename = "adjEq")]
    pub adj_eq: Option<String>,
    #[serde(rename = "details")]
    pub details: Vec<OkxBalanceDetail>,
    #[serde(rename = "imr")]
    pub imr: Option<String>,
    #[serde(rename = "isoEq")]
    pub iso_eq: Option<String>,
    #[serde(rename = "mgnRatio")]
    pub mgn_ratio: Option<String>,
    #[serde(rename = "mmr")]
    pub mmr: Option<String>,
    #[serde(rename = "notionalUsd")]
    pub notional_usd: Option<String>,
    #[serde(rename = "ordFroz")]
    pub ord_froz: Option<String>,
    #[serde(rename = "totalEq")]
    pub total_eq: String,
    #[serde(rename = "uTime")]
    pub u_time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OkxBalanceDetail {
    #[serde(rename = "availBal")]
    pub avail_bal: String,
    #[serde(rename = "availEq")]
    pub avail_eq: String,
    #[serde(rename = "cashBal")]
    pub cash_bal: String,
    #[serde(rename = "ccy")]
    pub ccy: String,
    #[serde(rename = "crossLiab")]
    pub cross_liab: String,
    #[serde(rename = "disEq")]
    pub dis_eq: String,
    #[serde(rename = "eq")]
    pub eq: String,
    #[serde(rename = "eqUsd")]
    pub eq_usd: String,
    #[serde(rename = "frozenBal")]
    pub frozen_bal: String,
    #[serde(rename = "interest")]
    pub interest: String,
    #[serde(rename = "isoEq")]
    pub iso_eq: String,
    #[serde(rename = "isoLiab")]
    pub iso_liab: String,
    #[serde(rename = "isoUpl")]
    pub iso_upl: String,
    #[serde(rename = "liab")]
    pub liab: String,
    #[serde(rename = "maxLoan")]
    pub max_loan: String,
    #[serde(rename = "mgnRatio")]
    pub mgn_ratio: String,
    #[serde(rename = "notionalLever")]
    pub notional_lever: String,
    #[serde(rename = "ordFrozen")]
    pub ord_frozen: String,
    #[serde(rename = "twap")]
    pub twap: String,
    #[serde(rename = "uTime")]
    pub u_time: String,
    #[serde(rename = "upl")]
    pub upl: String,
    #[serde(rename = "uplLiab")]
    pub upl_liab: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OkxOrderRequest {
    #[serde(rename = "instId")]
    pub inst_id: String,
    #[serde(rename = "tdMode")]
    pub td_mode: String,
    #[serde(rename = "side")]
    pub side: String,
    #[serde(rename = "ordType")]
    pub ord_type: String,
    #[serde(rename = "sz")]
    pub sz: String,
    #[serde(rename = "px", skip_serializing_if = "Option::is_none")]
    pub px: Option<String>,
    #[serde(rename = "ccy", skip_serializing_if = "Option::is_none")]
    pub ccy: Option<String>,
    #[serde(rename = "clOrdId", skip_serializing_if = "Option::is_none")]
    pub cl_ord_id: Option<String>,
    #[serde(rename = "tag", skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OkxOrderResponse {
    #[serde(rename = "clOrdId")]
    pub cl_ord_id: String,
    #[serde(rename = "ordId")]
    pub ord_id: String,
    #[serde(rename = "tag")]
    pub tag: String,
    #[serde(rename = "sCode")]
    pub s_code: String,
    #[serde(rename = "sMsg")]
    pub s_msg: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OkxPosition {
    #[serde(rename = "adl")]
    pub adl: String,
    #[serde(rename = "availPos")]
    pub avail_pos: String,
    #[serde(rename = "avgPx")]
    pub avg_px: String,
    #[serde(rename = "cTime")]
    pub c_time: String,
    #[serde(rename = "ccy")]
    pub ccy: String,
    #[serde(rename = "deltaBS")]
    pub delta_bs: String,
    #[serde(rename = "deltaPA")]
    pub delta_pa: String,
    #[serde(rename = "gammaBS")]
    pub gamma_bs: String,
    #[serde(rename = "gammaPA")]
    pub gamma_pa: String,
    #[serde(rename = "imr")]
    pub imr: String,
    #[serde(rename = "instId")]
    pub inst_id: String,
    #[serde(rename = "instType")]
    pub inst_type: String,
    #[serde(rename = "interest")]
    pub interest: String,
    #[serde(rename = "last")]
    pub last: String,
    #[serde(rename = "lever")]
    pub lever: String,
    #[serde(rename = "liab")]
    pub liab: String,
    #[serde(rename = "liabCcy")]
    pub liab_ccy: String,
    #[serde(rename = "liqPx")]
    pub liq_px: String,
    #[serde(rename = "markPx")]
    pub mark_px: String,
    #[serde(rename = "margin")]
    pub margin: String,
    #[serde(rename = "mgnMode")]
    pub mgn_mode: String,
    #[serde(rename = "mgnRatio")]
    pub mgn_ratio: String,
    #[serde(rename = "mmr")]
    pub mmr: String,
    #[serde(rename = "notionalUsd")]
    pub notional_usd: String,
    #[serde(rename = "optVal")]
    pub opt_val: String,
    #[serde(rename = "pTime")]
    pub p_time: String,
    #[serde(rename = "pos")]
    pub pos: String,
    #[serde(rename = "posCcy")]
    pub pos_ccy: String,
    #[serde(rename = "posId")]
    pub pos_id: String,
    #[serde(rename = "posSide")]
    pub pos_side: String,
    #[serde(rename = "thetaBS")]
    pub theta_bs: String,
    #[serde(rename = "thetaPA")]
    pub theta_pa: String,
    #[serde(rename = "tradeId")]
    pub trade_id: String,
    #[serde(rename = "uTime")]
    pub u_time: String,
    #[serde(rename = "upl")]
    pub upl: String,
    #[serde(rename = "uplRatio")]
    pub upl_ratio: String,
    #[serde(rename = "vegaBS")]
    pub vega_bs: String,
    #[serde(rename = "vegaPA")]
    pub vega_pa: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OkxApiResponse<T> {
    pub code: String,
    pub msg: String,
    pub data: Vec<T>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OkxWebSocketMessage {
    pub arg: Option<OkxWebSocketChannel>,
    pub data: Option<serde_json::Value>,
    pub event: Option<String>,
    pub code: Option<String>,
    pub msg: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OkxWebSocketChannel {
    pub channel: String,
    #[serde(rename = "instId")]
    pub inst_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OkxWebSocketSubscription {
    pub op: String,
    pub args: Vec<OkxWebSocketChannel>,
}

impl OkxTicker {
    pub fn to_decimal(&self, value: &str) -> rust_decimal::Decimal {
        value.parse().unwrap_or_default()
    }
    
    pub fn last_price(&self) -> Decimal {
        self.to_decimal(&self.last)
    }
    
    pub fn bid_price(&self) -> Decimal {
        self.to_decimal(&self.bid_px)
    }
    
    pub fn ask_price(&self) -> Decimal {
        self.to_decimal(&self.ask_px)
    }
    
    pub fn volume_24h(&self) -> Decimal {
        self.to_decimal(&self.vol_24h)
    }
    
    pub fn change_24h(&self) -> Decimal {
        let current = self.last_price();
        let open = self.to_decimal(&self.open_24h);
        if open.is_zero() {
            Decimal::ZERO
        } else {
            current - open
        }
    }
}

impl OkxOrderBook {
    pub fn best_bid(&self) -> Option<(Decimal, Decimal)> {
        self.bids.first().and_then(|bid| {
            if bid.len() >= 2 {
                let price = bid[0].parse().ok()?;
                let size = bid[1].parse().ok()?;
                Some((price, size))
            } else {
                None
            }
        })
    }
    
    pub fn best_ask(&self) -> Option<(Decimal, Decimal)> {
        self.asks.first().and_then(|ask| {
            if ask.len() >= 2 {
                let price = ask[0].parse().ok()?;
                let size = ask[1].parse().ok()?;
                Some((price, size))
            } else {
                None
            }
        })
    }
    
    pub fn spread(&self) -> Option<Decimal> {
        match (self.best_bid(), self.best_ask()) {
            (Some((bid_price, _)), Some((ask_price, _))) => Some(ask_price - bid_price),
            _ => None,
        }
    }
}