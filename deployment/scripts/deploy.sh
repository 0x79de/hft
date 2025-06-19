#!/bin/bash
# HFT Trading System Deployment Script

set -euo pipefail

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
DEPLOYMENT_DIR="${PROJECT_ROOT}/deployment"

# Default values
ENVIRONMENT="production"
BUILD_IMAGE=true
PUSH_IMAGE=true
DEPLOY_K8S=true
IMAGE_TAG="latest"
REGISTRY="your-registry.com"
NAMESPACE="hft"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Logging functions
log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Help function
show_help() {
    cat << EOF
HFT Trading System Deployment Script

Usage: $0 [OPTIONS]

Options:
    -e, --environment ENV     Target environment (production, staging, development)
    -t, --tag TAG            Docker image tag (default: latest)
    -r, --registry REGISTRY  Docker registry URL
    -n, --namespace NS       Kubernetes namespace (default: hft)
    --no-build              Skip Docker image build
    --no-push               Skip Docker image push
    --no-deploy             Skip Kubernetes deployment
    -h, --help              Show this help message

Examples:
    $0                                          # Deploy with defaults
    $0 -e staging -t v1.2.3                   # Deploy to staging with specific tag
    $0 --no-build --no-push -e development    # Deploy existing image to development

EOF
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -e|--environment)
            ENVIRONMENT="$2"
            shift 2
            ;;
        -t|--tag)
            IMAGE_TAG="$2"
            shift 2
            ;;
        -r|--registry)
            REGISTRY="$2"
            shift 2
            ;;
        -n|--namespace)
            NAMESPACE="$2"
            shift 2
            ;;
        --no-build)
            BUILD_IMAGE=false
            shift
            ;;
        --no-push)
            PUSH_IMAGE=false
            shift
            ;;
        --no-deploy)
            DEPLOY_K8S=false
            shift
            ;;
        -h|--help)
            show_help
            exit 0
            ;;
        *)
            log_error "Unknown option: $1"
            show_help
            exit 1
            ;;
    esac
done

# Validate environment
if [[ ! "$ENVIRONMENT" =~ ^(production|staging|development)$ ]]; then
    log_error "Invalid environment: $ENVIRONMENT"
    exit 1
fi

# Set image name
IMAGE_NAME="${REGISTRY}/hft/trading-system"
FULL_IMAGE_NAME="${IMAGE_NAME}:${IMAGE_TAG}"

log_info "Starting deployment for environment: $ENVIRONMENT"
log_info "Image: $FULL_IMAGE_NAME"
log_info "Namespace: $NAMESPACE"

# Pre-deployment checks
check_prerequisites() {
    log_info "Checking prerequisites..."
    
    # Check if required tools are installed
    local tools=("docker" "kubectl" "helm")
    for tool in "${tools[@]}"; do
        if ! command -v "$tool" &> /dev/null; then
            log_error "$tool is not installed or not in PATH"
            exit 1
        fi
    done
    
    # Check Docker daemon
    if ! docker info &> /dev/null; then
        log_error "Docker daemon is not running"
        exit 1
    fi
    
    # Check Kubernetes connection
    if ! kubectl cluster-info &> /dev/null; then
        log_error "Cannot connect to Kubernetes cluster"
        exit 1
    fi
    
    log_info "Prerequisites check passed"
}

# Build Docker image
build_image() {
    if [ "$BUILD_IMAGE" = true ]; then
        log_info "Building Docker image..."
        
        cd "$PROJECT_ROOT"
        
        # Run tests before building
        log_info "Running tests..."
        CARGO_TEST=1 cargo test --all --release
        
        # Build the image
        docker build \
            -f "${DEPLOYMENT_DIR}/docker/Dockerfile" \
            -t "$FULL_IMAGE_NAME" \
            --build-arg BUILD_ENV="$ENVIRONMENT" \
            .
        
        log_info "Docker image built successfully: $FULL_IMAGE_NAME"
    else
        log_info "Skipping Docker image build"
    fi
}

# Push Docker image
push_image() {
    if [ "$PUSH_IMAGE" = true ]; then
        log_info "Pushing Docker image to registry..."
        
        # Login to registry (assumes registry auth is configured)
        docker push "$FULL_IMAGE_NAME"
        
        log_info "Docker image pushed successfully"
    else
        log_info "Skipping Docker image push"
    fi
}

# Deploy to Kubernetes
deploy_kubernetes() {
    if [ "$DEPLOY_K8S" = true ]; then
        log_info "Deploying to Kubernetes..."
        
        # Create namespace if it doesn't exist
        kubectl create namespace "$NAMESPACE" --dry-run=client -o yaml | kubectl apply -f -
        
        # Apply Kubernetes manifests
        local k8s_dir="${DEPLOYMENT_DIR}/kubernetes"
        
        # Update image in deployment
        sed "s|image: hft/trading-system:1.0.0|image: ${FULL_IMAGE_NAME}|g" \
            "${k8s_dir}/deployment.yaml" | \
            kubectl apply -n "$NAMESPACE" -f -
        
        # Wait for deployment to be ready
        log_info "Waiting for deployment to be ready..."
        kubectl wait --for=condition=available --timeout=300s \
            deployment/hft-trading-system -n "$NAMESPACE"
        
        # Get service information
        kubectl get services -n "$NAMESPACE"
        
        log_info "Kubernetes deployment completed successfully"
    else
        log_info "Skipping Kubernetes deployment"
    fi
}

# Deploy monitoring stack
deploy_monitoring() {
    log_info "Deploying monitoring stack..."
    
    local monitoring_dir="${DEPLOYMENT_DIR}/monitoring"
    
    # Deploy Prometheus
    helm repo add prometheus-community https://prometheus-community.github.io/helm-charts
    helm repo update
    
    helm upgrade --install prometheus prometheus-community/kube-prometheus-stack \
        --namespace monitoring \
        --create-namespace \
        --values "${monitoring_dir}/prometheus-values.yaml" \
        --wait
    
    # Apply custom Prometheus config
    kubectl create configmap prometheus-config \
        --from-file="${monitoring_dir}/prometheus.yml" \
        --namespace monitoring \
        --dry-run=client -o yaml | kubectl apply -f -
    
    # Apply alerting rules
    kubectl create configmap hft-alerts \
        --from-file="${monitoring_dir}/alerts/" \
        --namespace monitoring \
        --dry-run=client -o yaml | kubectl apply -f -
    
    # Import Grafana dashboard
    kubectl create configmap hft-dashboard \
        --from-file="${monitoring_dir}/grafana/dashboards/" \
        --namespace monitoring \
        --dry-run=client -o yaml | kubectl apply -f -
    
    log_info "Monitoring stack deployed successfully"
}

# Post-deployment verification
verify_deployment() {
    log_info "Verifying deployment..."
    
    # Check pod status
    kubectl get pods -n "$NAMESPACE" -l app=hft-trading
    
    # Check service endpoints
    kubectl get endpoints -n "$NAMESPACE"
    
    # Test health endpoint
    local service_ip=$(kubectl get service hft-trading-service -n "$NAMESPACE" -o jsonpath='{.spec.clusterIP}')
    if kubectl run test-pod --rm -i --restart=Never --image=curlimages/curl -- \
        curl -f "http://${service_ip}:8080/health" &> /dev/null; then
        log_info "Health check passed"
    else
        log_warn "Health check failed - service may still be starting"
    fi
    
    # Display useful information
    log_info "Deployment verification completed"
    log_info "To access the application:"
    log_info "  kubectl port-forward service/hft-trading-service 8080:8080 -n $NAMESPACE"
    log_info "To view logs:"
    log_info "  kubectl logs -l app=hft-trading -n $NAMESPACE -f"
}

# Cleanup function
cleanup() {
    log_info "Cleaning up temporary resources..."
    # Add any cleanup logic here
}

# Error handling
handle_error() {
    log_error "Deployment failed at step: ${1:-unknown}"
    cleanup
    exit 1
}

# Main deployment flow
main() {
    # Set error trap
    trap 'handle_error ${LINENO}' ERR
    
    # Run deployment steps
    check_prerequisites
    build_image
    push_image
    deploy_kubernetes
    
    if [ "$ENVIRONMENT" = "production" ]; then
        deploy_monitoring
    fi
    
    verify_deployment
    
    log_info "Deployment completed successfully!"
    log_info "Environment: $ENVIRONMENT"
    log_info "Image: $FULL_IMAGE_NAME"
    log_info "Namespace: $NAMESPACE"
}

# Run main function
main "$@"