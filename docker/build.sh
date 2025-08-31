#!/bin/bash
# Build and manage Codetriever Docker images

set -e

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}Building Codetriever Docker images...${NC}"
echo

# Build distroless image (default, most secure)
echo -e "${GREEN}Building distroless image (53MB, ultra-secure)...${NC}"
docker build -f docker/Dockerfile.api -t codetriever-api:distroless -t codetriever-api:latest .

# Build Chainguard glibc image (smallest)
echo -e "${GREEN}Building Chainguard image (19MB, minimal)...${NC}"
docker build -f docker/Dockerfile.api.chainguard -t codetriever-api:chainguard .

echo
echo -e "${BLUE}Build complete! Available images:${NC}"
docker images | head -1
docker images | grep codetriever-api | sort

echo
echo -e "${BLUE}To run:${NC}"
echo "  docker run -d --name codetriever -p 8080:8080 codetriever-api:latest"
echo "  docker run -d --name codetriever-minimal -p 8080:8080 codetriever-api:chainguard"