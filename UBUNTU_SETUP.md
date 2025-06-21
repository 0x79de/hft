# Ubuntu Setup Guide for HFT Trading System

## System Requirements ✅

Your hardware is excellent for HFT trading:
- **CPU**: Intel i7-12700H (12 cores, 20 threads) - Perfect for high-frequency trading
- **RAM**: 16GB - More than sufficient for order processing and market data
- **Storage**: SSD recommended for optimal performance
- **Network**: Low-latency internet connection for exchange connectivity

## Ubuntu Installation & Setup

### 1. System Preparation

```bash
# Update your Ubuntu system
sudo apt update && sudo apt upgrade -y

# Install essential build tools
sudo apt install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    libffi-dev \
    curl \
    wget \
    git \
    htop \
    net-tools \
    iperf3

# Install additional performance tools
sudo apt install -y \
    linux-tools-common \
    linux-tools-generic \
    stress-ng \
    sysstat
```

### 2. Rust Installation

```bash
# Install Rust with stable toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Configure current shell
source ~/.cargo/env

# Verify installation
rustc --version
cargo --version

# Install additional Rust tools
cargo install cargo-watch cargo-expand cargo-audit
```

### 3. Performance Optimization

#### CPU Optimization
```bash
# Set CPU governor to performance mode
echo 'performance' | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor

# Make permanent by adding to /etc/rc.local
echo 'echo performance | tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor' | sudo tee -a /etc/rc.local
sudo chmod +x /etc/rc.local
```

#### Memory Optimization
```bash
# Increase memory limits for trading applications
echo 'fs.file-max = 2097152' | sudo tee -a /etc/sysctl.conf
echo 'vm.swappiness = 1' | sudo tee -a /etc/sysctl.conf
echo 'net.core.rmem_max = 134217728' | sudo tee -a /etc/sysctl.conf
echo 'net.core.wmem_max = 134217728' | sudo tee -a /etc/sysctl.conf

# Apply changes
sudo sysctl -p
```

#### Network Optimization
```bash
# Optimize network for low latency
echo 'net.ipv4.tcp_congestion_control = bbr' | sudo tee -a /etc/sysctl.conf
echo 'net.core.default_qdisc = fq' | sudo tee -a /etc/sysctl.conf
echo 'net.ipv4.tcp_slow_start_after_idle = 0' | sudo tee -a /etc/sysctl.conf

# Apply network optimizations
sudo sysctl -p
```

### 4. Build Configuration

#### Create optimal build configuration
```bash
# Create .cargo/config.toml for project-specific optimizations
mkdir -p .cargo
cat > .cargo/config.toml << 'EOF'
[build]
rustflags = [
    "-C", "target-cpu=native",
    "-C", "opt-level=3",
    "-C", "lto=fat",
    "-C", "codegen-units=1",
    "-C", "panic=abort"
]

[target.x86_64-unknown-linux-gnu]
rustflags = ["-C", "link-arg=-fuse-ld=lld"]
EOF

# Install LLD linker for faster builds
sudo apt install -y lld
```

#### Environment Variables for Performance
```bash
# Add to ~/.bashrc for permanent settings
cat >> ~/.bashrc << 'EOF'

# HFT Trading System Environment
export RUSTFLAGS="-C target-cpu=native -C opt-level=3"
export CARGO_TARGET_DIR="/tmp/hft-target"  # Use tmpfs for faster builds
export RUST_LOG=info
export MIMALLOC_LARGE_OS_PAGES=1  # Use huge pages if available

# OKX Integration Environment
export OKX_SANDBOX=true
export OKX_TIMEOUT_MS=3000
export OKX_RATE_LIMIT_RPS=20
EOF

source ~/.bashrc
```

### 5. Building the Project

```bash
# Clean any previous builds
cargo clean

# Build with maximum optimizations
cargo build --release --features integrations

# Verify the binary
ls -la target/release/hft
file target/release/hft

# Check binary size and optimization
strip target/release/hft  # Remove debug symbols
ls -lh target/release/hft
```

### 6. Performance Testing

#### System Benchmarks
```bash
# CPU benchmark
stress-ng --cpu $(nproc) --timeout 60s --metrics-brief

# Memory bandwidth test
sudo apt install -y mbw
mbw 1024

# Network latency test (to OKX)
ping -c 10 aws-ap-northeast-1.okx.com
```

#### Application Benchmarks
```bash
# Run built-in benchmarks
cargo bench --features integrations

# Test with sample configuration
./target/release/hft --help

# Run with performance monitoring
perf record ./target/release/hft
perf report
```

### 7. Production Deployment

#### System Service Setup
```bash
# Create systemd service file
sudo tee /etc/systemd/system/hft-trading.service << 'EOF'
[Unit]
Description=HFT Trading System
After=network.target

[Service]
Type=simple
User=your_username
WorkingDirectory=/path/to/hft
ExecStart=/path/to/hft/target/release/hft
Restart=always
RestartSec=5
Environment=RUST_LOG=info
Environment=OKX_SANDBOX=false

# Security settings
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/path/to/hft

[Install]
WantedBy=multi-user.target
EOF

# Enable and start service
sudo systemctl daemon-reload
sudo systemctl enable hft-trading
sudo systemctl start hft-trading

# Check status
sudo systemctl status hft-trading
```

#### Monitoring Setup
```bash
# Install monitoring tools
sudo apt install -y prometheus-node-exporter grafana

# Create monitoring script
cat > monitor_hft.sh << 'EOF'
#!/bin/bash
while true; do
    echo "=== $(date) ==="
    echo "CPU Usage: $(top -bn1 | grep "Cpu(s)" | awk '{print $2}' | cut -d'%' -f1)"
    echo "Memory: $(free -h | grep Mem | awk '{print $3 "/" $2}')"
    echo "HFT Process: $(ps aux | grep hft | grep -v grep)"
    echo "Network: $(ss -tuln | grep :8080)"
    sleep 30
done
EOF

chmod +x monitor_hft.sh
```

### 8. Security Configuration

```bash
# Create dedicated user for trading
sudo useradd -r -s /bin/false hft-trader
sudo usermod -aG hft-trader your_username

# Set file permissions
sudo chown -R hft-trader:hft-trader /path/to/hft
sudo chmod 600 integration-config.toml

# Firewall configuration
sudo ufw enable
sudo ufw allow ssh
sudo ufw allow out 443/tcp  # HTTPS for OKX API
sudo ufw allow out 53/udp   # DNS
```

## Performance Expectations on Your Hardware

### Expected Performance Metrics
- **Order Processing**: 1-2 million orders/second
- **Latency**: <100 nanoseconds for order matching
- **Memory Usage**: 200-500MB typical
- **CPU Usage**: 20-40% under normal load
- **Network Latency**: 10-50ms to OKX (depending on location)

### Optimization Tips

1. **Use Performance CPU Governor**
   ```bash
   sudo cpupower frequency-set -g performance
   ```

2. **Enable Huge Pages**
   ```bash
   echo 'vm.nr_hugepages = 128' | sudo tee -a /etc/sysctl.conf
   ```

3. **Optimize for Your CPU**
   ```bash
   export RUSTFLAGS="-C target-cpu=native -C target-feature=+avx2"
   ```

4. **Use Dedicated Network Interface**
   - Consider USB-to-Ethernet adapter for dedicated trading connection
   - Configure Quality of Service (QoS) for trading traffic

## Troubleshooting

### Common Issues

1. **Permission Denied**
   ```bash
   sudo chown -R $USER:$USER ~/.cargo
   ```

2. **SSL/TLS Errors**
   ```bash
   sudo apt install -y ca-certificates
   sudo update-ca-certificates
   ```

3. **Network Connectivity**
   ```bash
   # Test OKX connectivity
   curl -I https://www.okx.com/api/v5/public/time
   ```

4. **Memory Issues**
   ```bash
   # Check memory usage
   free -h
   sudo sysctl vm.drop_caches=3
   ```

### Performance Monitoring

```bash
# Real-time system monitoring
htop

# Network monitoring
sudo nethogs

# Disk I/O monitoring
sudo iotop

# Application-specific monitoring
cargo flamegraph --bin hft  # Requires cargo-flamegraph
```

## Ubuntu-Specific Advantages

1. **Low Latency Kernel**: Use `linux-lowlatency` for better real-time performance
2. **Package Management**: Easy dependency installation with apt
3. **Container Support**: Docker/Podman for isolated environments
4. **Professional Tools**: Access to professional trading and monitoring tools
5. **Community Support**: Large Ubuntu community for troubleshooting

Your i7-12700H with 16GB RAM on Ubuntu will provide excellent performance for high-frequency trading! 🚀