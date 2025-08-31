.PHONY: build push deploy clean logs

# Docker registry (customize this)
REGISTRY ?= codetriever
VERSION ?= $(shell git describe --tags --always --dirty)
API_IMAGE = $(REGISTRY)/api:$(VERSION)
API_IMAGE_LATEST = $(REGISTRY)/api:latest

# Build the API Docker image
build:
	@echo "🔨 Building API image: $(API_IMAGE)"
	docker build -f Dockerfile.api -t $(API_IMAGE) -t $(API_IMAGE_LATEST) ..

# Push to registry
push: build
	@echo "📤 Pushing images to registry"
	docker push $(API_IMAGE)
	docker push $(API_IMAGE_LATEST)

# Deploy with docker-compose
deploy:
	@echo "🚀 Deploying Codetriever stack"
	docker-compose -f ../docker-compose.prod.yml up -d

# Stop deployment
stop:
	@echo "🛑 Stopping Codetriever stack"
	docker-compose -f ../docker-compose.prod.yml down

# Clean up
clean:
	@echo "🧹 Cleaning up Docker resources"
	docker-compose -f ../docker-compose.prod.yml down -v
	docker rmi $(API_IMAGE) $(API_IMAGE_LATEST) || true

# View logs
logs:
	docker-compose -f ../docker-compose.prod.yml logs -f

# Health check
health:
	@echo "🏥 Checking service health"
	@curl -s http://localhost:6333/health | jq '.' || echo "Qdrant: ❌"
	@curl -s http://localhost:8080/health | jq '.' || echo "API: ❌"