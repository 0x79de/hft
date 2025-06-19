# HFT Trading System - Production Deployment Guide

## Overview

This document provides comprehensive instructions for deploying the HFT Trading System in production environments using Docker and Kubernetes, complete with monitoring and alerting.

## Architecture

The production deployment consists of:

- **HFT Trading System**: Main application with multiple replicas
- **Prometheus**: Metrics collection and alerting
- **Grafana**: Dashboards and visualization
- **Redis**: Caching and session storage
- **Load Balancer**: Traffic distribution and high availability

## Prerequisites

### System Requirements

- **CPU**: Minimum 8 cores, recommended 16+ cores
- **Memory**: Minimum 16GB RAM, recommended 32GB+
- **Storage**: SSD with minimum 100GB, recommended 500GB+
- **Network**: Low-latency network (sub-millisecond preferred)

### Software Requirements

- Docker 20.10+
- Kubernetes 1.24+
- Helm 3.8+
- kubectl configured for target cluster

### Environment Setup

```bash
# Install required tools
curl -fsSL https://get.docker.com | sh
curl -fsSL https://raw.githubusercontent.com/helm/helm/main/scripts/get-helm-3 | bash

# Verify installations
docker --version
kubectl version --client
helm version
```

## Deployment Methods

### 1. Docker Compose (Development/Testing)

```bash
# Clone repository
git clone <repository-url>
cd hft/deployment/docker

# Start services
docker-compose up -d

# View logs
docker-compose logs -f hft-trading

# Scale trading instances
docker-compose up -d --scale hft-trading=3

# Stop services
docker-compose down
```

### 2. Kubernetes (Production)

#### Quick Deployment

```bash
# Run automated deployment script
./deployment/scripts/deploy.sh --environment production --tag v1.0.0

# Manual deployment
kubectl apply -f deployment/kubernetes/
```

#### Step-by-Step Deployment

1. **Create Namespace**
   ```bash
   kubectl create namespace hft
   ```

2. **Deploy Configuration**
   ```bash
   kubectl apply -f deployment/kubernetes/deployment.yaml
   ```

3. **Verify Deployment**
   ```bash
   kubectl get pods -n hft
   kubectl get services -n hft
   ```

4. **Check Health**
   ```bash
   kubectl port-forward service/hft-trading-service 8080:8080 -n hft
   curl http://localhost:8080/health
   ```

## Configuration

### Environment-Specific Configurations

#### Production (`production.toml`)
- High-performance settings
- Full monitoring enabled
- Security hardened
- Auto-scaling enabled

#### Staging (`staging.toml`)
- Production-like configuration
- Reduced resource limits
- Enhanced logging
- Test data enabled

#### Development (`development.toml`)
- Debug mode enabled
- Mock data sources
- Local storage
- Relaxed security

### Key Configuration Parameters

```toml
[system]
max_workers = 8                    # CPU cores to utilize
worker_queue_size = 100000         # Queue size for orders

[trading]
max_orders_per_second = 100000     # Throughput limit
enabled_symbols = ["BTCUSD", "ETHUSD"]

[latency_profiling]
enabled = true                     # Enable performance monitoring
sample_rate = 1.0                  # Sample 100% in production

[monitoring]
metrics_port = 8080               # Prometheus metrics port
enable_prometheus = true
```

## Monitoring and Alerting

### Metrics Collection

The system exposes metrics on port 8080 at `/metrics` endpoint:

- **Trading Metrics**: Order latency, throughput, error rates
- **System Metrics**: CPU, memory, network usage
- **Business Metrics**: PnL, positions, risk utilization

### Key Performance Indicators (KPIs)

| Metric | Threshold | Alert Level |
|--------|-----------|-------------|
| Order Latency P99 | < 10μs | Critical |
| Throughput | > 50K ops/sec | Warning |
| Error Rate | < 0.1% | Warning |
| Memory Usage | < 85% | Warning |
| Risk Utilization | < 90% | Critical |

### Dashboard Access

```bash
# Forward Grafana port
kubectl port-forward service/grafana 3000:3000 -n monitoring

# Access dashboard
open http://localhost:3000
# Default login: admin/admin
```

### Alert Configuration

Alerts are automatically configured for:

- System downtime
- High latency (>10μs)
- Risk limit breaches
- Memory/CPU exhaustion
- Market data feed issues

## Security

### Container Security

- Non-root user execution
- Read-only root filesystem
- Dropped capabilities
- Security contexts enforced

### Network Security

- TLS encryption for all communications
- Network policies for traffic isolation
- Service mesh for zero-trust networking

### Access Control

- RBAC for Kubernetes resources
- JWT authentication for API access
- Audit logging enabled

## High Availability

### Replication Strategy

- Minimum 2 replicas per service
- Anti-affinity rules for pod distribution
- Graceful shutdown handling

### Load Balancing

- Kubernetes Services for internal traffic
- Ingress controllers for external access
- Health-based routing

### Disaster Recovery

- Automated backups every 6 hours
- Cross-region replication
- RTO: 5 minutes, RPO: 1 minute

## Performance Tuning

### Kubernetes Resource Allocation

```yaml
resources:
  requests:
    cpu: "2"
    memory: "4Gi"
  limits:
    cpu: "4"
    memory: "8Gi"
```

### System-Level Optimizations

```bash
# CPU affinity
echo 'isolated_cores=0-3' >> /etc/default/grub

# Memory huge pages
echo 'vm.nr_hugepages=1024' >> /etc/sysctl.conf

# Network optimizations
echo 'net.core.rmem_max=134217728' >> /etc/sysctl.conf
echo 'net.core.wmem_max=134217728' >> /etc/sysctl.conf
```

### JVM Tuning (if applicable)

```bash
export JAVA_OPTS="-Xms4g -Xmx8g -XX:+UseG1GC -XX:MaxGCPauseMillis=200"
```

## Troubleshooting

### Common Issues

1. **High Latency**
   ```bash
   # Check CPU affinity
   kubectl describe node <node-name>
   
   # Check network latency
   kubectl exec -it <pod-name> -- ping <target>
   ```

2. **Memory Issues**
   ```bash
   # Check memory usage
   kubectl top pods -n hft
   
   # Analyze memory leaks
   kubectl logs <pod-name> -n hft | grep -i memory
   ```

3. **Order Book Corruption**
   ```bash
   # Check order book state
   curl http://localhost:8080/debug/orderbook/BTCUSD
   
   # Restart trading engine
   kubectl rollout restart deployment/hft-trading-system -n hft
   ```

### Log Analysis

```bash
# View aggregated logs
kubectl logs -l app=hft-trading -n hft --tail=1000

# Stream live logs
kubectl logs -f deployment/hft-trading-system -n hft

# Export logs for analysis
kubectl logs deployment/hft-trading-system -n hft > hft-logs.txt
```

### Performance Debugging

```bash
# Check metrics endpoint
curl -s http://localhost:8080/metrics | grep hft_

# Profile CPU usage
kubectl exec -it <pod-name> -- perf top

# Analyze network traffic
kubectl exec -it <pod-name> -- ss -tulpn
```

## Maintenance

### Updating the Application

```bash
# Rolling update
kubectl set image deployment/hft-trading-system \
  hft-trading=hft/trading-system:v1.1.0 -n hft

# Monitor rollout
kubectl rollout status deployment/hft-trading-system -n hft

# Rollback if needed
kubectl rollout undo deployment/hft-trading-system -n hft
```

### Scaling Operations

```bash
# Scale replicas
kubectl scale deployment hft-trading-system --replicas=5 -n hft

# Auto-scaling based on CPU
kubectl autoscale deployment hft-trading-system \
  --cpu-percent=70 --min=2 --max=10 -n hft
```

### Backup and Recovery

```bash
# Backup configuration
kubectl get configmaps -n hft -o yaml > hft-config-backup.yaml

# Backup persistent data
kubectl exec -it <pod-name> -- tar -czf /tmp/backup.tar.gz /app/data

# Restore from backup
kubectl apply -f hft-config-backup.yaml
```

## Contact and Support

- **Development Team**: dev-team@company.com
- **Operations Team**: ops-team@company.com
- **Emergency Hotline**: +1-XXX-XXX-XXXX
- **Documentation**: https://docs.company.com/hft
- **Issue Tracker**: https://github.com/company/hft/issues

## Additional Resources

- [API Documentation](./API.md)
- [Architecture Guide](./ARCHITECTURE.md)
- [Performance Benchmarks](./PERFORMANCE.md)
- [Security Guidelines](./SECURITY.md)
- [Development Setup](./README.md)