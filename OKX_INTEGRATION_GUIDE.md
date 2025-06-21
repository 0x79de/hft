# OKX Live Trading Integration Guide

## Overview

This guide explains how to integrate your HFT trading system with OKX exchange for live cryptocurrency trading. The integration provides:

- **REST API Client** for order placement and account queries
- **WebSocket Client** for real-time market data
- **HMAC-SHA256 Authentication** for secure API access
- **Automatic Rate Limiting** and error handling
- **Real-time Market Data Processing** 

## Prerequisites

1. **OKX Account**: Create an account at [OKX.com](https://www.okx.com)
2. **API Credentials**: Generate API keys with trading permissions
3. **Rust Environment**: Ensure you have Rust 1.70+ installed

## Step 1: Get OKX API Credentials

### Create API Key

1. Login to your OKX account
2. Go to **Account Settings** → **API Management**
3. Click **Create API Key**
4. Set permissions:
   - ✅ **Read**: View account information
   - ✅ **Trade**: Place and cancel orders
   - ✅ **Withdraw**: (Optional) For fund transfers
5. Set IP restrictions for security
6. Enable 2FA verification

### Save Your Credentials

You'll receive:
- **API Key**: `your_api_key_here`
- **Secret Key**: `your_secret_key_here` (Base64 encoded)
- **Passphrase**: `your_passphrase_here`

⚠️ **IMPORTANT**: Keep these credentials secure and never commit them to version control.

## Step 2: Configure Environment

### Option A: Environment Variables (Recommended)

```bash
# OKX Configuration
export OKX_API_KEY="your_api_key_here"
export OKX_SECRET_KEY="your_secret_key_here"
export OKX_PASSPHRASE="your_passphrase_here"
export OKX_SANDBOX="true"  # Set to false for production

# Optional: Advanced Configuration
export OKX_TIMEOUT_MS="5000"
export OKX_RATE_LIMIT_RPS="20"
```

### Option B: Configuration File

Create `integration-config.toml`:

```toml
[okx]
api_key = "your_api_key_here"
secret_key = "your_secret_key_here"
passphrase = "your_passphrase_here"
sandbox = true
timeout_ms = 5000
rate_limit_requests_per_second = 20

[mcp]
server_url = "http://localhost:8000"
timeout_ms = 1000

[rag]
server_url = "http://localhost:8001"
timeout_ms = 500

[coordinator]
signal_processing_interval_ms = 100
health_check_interval_ms = 5000
```

## Step 3: Build and Run

### Build with Integration Support

```bash
cargo build --release --features integrations
```

### Run with OKX Integration

```bash
# Using environment variables
cargo run --release --features integrations

# Using configuration file
CONFIG_FILE=integration-config.toml cargo run --release --features integrations
```

## Step 4: Testing Your Integration

### 1. Test API Connection

```bash
# Check if your credentials work
curl -X GET "https://www.okx.com/api/v5/account/balance" \
  -H "OK-ACCESS-KEY: your_api_key" \
  -H "OK-ACCESS-SIGN: signature" \
  -H "OK-ACCESS-TIMESTAMP: timestamp" \
  -H "OK-ACCESS-PASSPHRASE: your_passphrase"
```

### 2. Start with Sandbox Mode

Always test with `OKX_SANDBOX=true` first:

```bash
export OKX_SANDBOX=true
cargo run --release --features integrations
```

### 3. Monitor Logs

Watch for successful integration startup:

```
INFO  Loading OKX integration with environment configuration
INFO  OKX integration initialized successfully
INFO  Starting OKX integration...
INFO  Setting up OKX market data subscriptions...
INFO  Subscribed to market data for BTC-USDT
INFO  OKX integration started successfully
```

## Step 5: Live Trading Features

### Market Data Streaming

The system automatically subscribes to:
- **Ticker Data**: Real-time price updates
- **Order Book**: Top 5 levels (bids/asks)
- **Trade Data**: Recent trade executions
- **Order Updates**: Your order status changes

### Supported Trading Operations

```rust
// Place a market buy order
execute_okx_trade("BTC-USDT", "buy", "0.001", None).await?;

// Place a limit sell order
execute_okx_trade("BTC-USDT", "sell", "0.001", Some("45000.0")).await?;

// Get account balance
let balance = okx.client.get_account_balance().await?;

// Get current positions
let positions = okx.client.get_positions(Some("BTC-USDT")).await?;

// Check market data
let ticker = okx.client.get_ticker("BTC-USDT").await?;
```

### Risk Management Integration

The OKX integration works with your existing risk management:

```rust
// Risk limits are automatically checked before placing orders
let btc_limits = RiskLimits::with_custom_limits(
    "BTC-USDT".to_string(),
    10.0,      // position limit
    50_000.0,  // daily pnl limit
    5.0,       // order size limit
    2.0,       // price deviation limit
    500_000.0, // notional limit
);
```

## Step 6: Production Deployment

### Security Checklist

- [ ] API keys stored securely (environment variables or encrypted config)
- [ ] IP whitelist configured on OKX
- [ ] 2FA enabled on OKX account
- [ ] Sandbox testing completed successfully
- [ ] Risk limits properly configured
- [ ] Monitoring and alerting set up

### Performance Optimization

```bash
# Use release build for maximum performance
cargo build --release --features integrations

# Set appropriate limits
export OKX_RATE_LIMIT_RPS="20"  # Respect OKX rate limits
export OKX_TIMEOUT_MS="3000"    # Fast timeout for HFT
```

### Monitoring

Monitor these key metrics:

- **API Response Times**: Should be < 100ms
- **WebSocket Connection**: Must stay connected
- **Order Fill Rates**: Track execution success
- **Error Rates**: Monitor for API errors
- **Rate Limiting**: Ensure you don't exceed limits

## Step 7: Advanced Features

### Custom Trading Strategies

```rust
// Example: Simple momentum strategy
async fn momentum_strategy(&self, symbol: &str) -> anyhow::Result<()> {
    let market_data = self.okx_integration?.get_market_context(symbol).await?;
    
    if market_data.change_24h > Decimal::from_f64(0.05).unwrap() {
        // Price up 5%, place buy order
        self.execute_okx_trade(symbol, "buy", "0.001", None).await?;
    } else if market_data.change_24h < Decimal::from_f64(-0.05).unwrap() {
        // Price down 5%, place sell order
        self.execute_okx_trade(symbol, "sell", "0.001", None).await?;
    }
    
    Ok(())
}
```

### WebSocket Event Handling

```rust
// Custom market data processing
async fn process_okx_market_data(data: &serde_json::Value) {
    if let Some(ticker_data) = data.get("data") {
        // Update your internal order book
        // Trigger trading signals
        // Calculate technical indicators
        // Execute trades based on strategy
    }
}
```

## Troubleshooting

### Common Issues

1. **Authentication Failed**
   - Check API key, secret, and passphrase
   - Ensure timestamp is correct
   - Verify signature generation

2. **Rate Limit Exceeded**
   - Reduce `OKX_RATE_LIMIT_RPS`
   - Implement exponential backoff
   - Monitor API usage

3. **WebSocket Disconnections**
   - Check network connectivity
   - Implement automatic reconnection
   - Monitor connection health

4. **Order Placement Fails**
   - Check account balance
   - Verify symbol format (e.g., "BTC-USDT")
   - Ensure trading permissions

### Debug Mode

Enable debug logging:

```bash
RUST_LOG=debug cargo run --release --features integrations
```

### Health Checks

The system includes built-in health checks:

```rust
// Check OKX API status
let health = okx.health_check().await?;
match health {
    HealthStatus::Healthy => info!("OKX API is healthy"),
    HealthStatus::Degraded => warn!("OKX API is slow"),
    HealthStatus::Unhealthy => error!("OKX API is down"),
}
```

## Support and Resources

- **OKX API Documentation**: https://www.okx.com/docs-v5/
- **OKX API Rate Limits**: https://www.okx.com/docs-v5/en/#overview-api-rate-limit
- **WebSocket Channels**: https://www.okx.com/docs-v5/en/#overview-websocket

## Security Best Practices

1. **Never hardcode credentials** in source code
2. **Use environment variables** or encrypted configuration
3. **Enable IP whitelisting** on OKX
4. **Monitor for unusual activity** in your trading account
5. **Keep API keys secure** and rotate them regularly
6. **Use sandbox mode** for testing
7. **Implement proper error handling** for all API calls
8. **Set up alerts** for failed trades or connectivity issues

---

## Quick Start Example

```bash
# 1. Set up credentials
export OKX_API_KEY="your_api_key_here"
export OKX_SECRET_KEY="your_secret_key_here"
export OKX_PASSPHRASE="your_passphrase_here"
export OKX_SANDBOX="true"

# 2. Build and run
cargo build --release --features integrations
cargo run --release --features integrations

# 3. Watch the logs for successful connection
# You should see market data streaming and be ready to trade!
```

Your HFT system is now integrated with OKX for live cryptocurrency trading! 🚀