# Security Configuration Guide

## Database Credentials

**⚠️ IMPORTANT: Never use default credentials in production!**

### Local Development Setup

1. **Initial Setup**
   ```bash
   # This will create .env from .env.sample if it doesn't exist
   just dev-setup
   ```

2. **Configure Credentials**
   - Edit `.env` file with your preferred credentials
   - Default local development credentials are provided for convenience
   - These match the docker-compose defaults for easy local testing

### Production Deployment

**NEVER use the default credentials in production!**

1. **Set Environment Variables**
   ```bash
   export DB_HOST=your-database-host
   export DB_PORT=5432
   export DB_NAME=codetriever
   export DB_USER=secure_user
   export DB_PASSWORD=strong_password
   export DB_SSLMODE=require
   ```

2. **Required Environment Variables**
   - `DB_HOST` - Database host (REQUIRED)
   - `DB_PORT` - Database port (REQUIRED)
   - `DB_NAME` - Database name (REQUIRED)
   - `DB_USER` - Database user (REQUIRED)
   - `DB_PASSWORD` - Database password (REQUIRED)
   - `DB_SSLMODE` - SSL mode for connections (optional, defaults to 'prefer')
   - `QDRANT_URL` - Qdrant vector database URL
   - `EMBEDDING_MODEL` - Model for code embeddings

3. **Security Best Practices**
   - Use strong, unique passwords (minimum 16 characters)
   - Enable SSL/TLS for database connections (`sslmode=require`)
   - Store credentials in secure secret management systems:
     - AWS Secrets Manager
     - HashiCorp Vault
     - Kubernetes Secrets
     - Azure Key Vault
   - Never commit `.env` files to version control
   - Rotate credentials regularly
   - Use separate credentials for each environment

### Docker Deployment

The docker-compose file supports environment variable substitution:

```yaml
environment:
  - POSTGRES_USER=${POSTGRES_USER:-fallback}
  - POSTGRES_PASSWORD=${POSTGRES_PASSWORD:-fallback}
  - POSTGRES_DB=${POSTGRES_DB:-fallback}
```

Pass credentials via environment or `.env` file:
```bash
# Using .env file (automatically loaded by docker-compose)
docker-compose up

# Or explicitly with environment variables
POSTGRES_PASSWORD=secure_password docker-compose up
```

### Application Behavior

The application will **fail fast** if database credentials are not set:
- No hardcoded credentials in the code
- Clear error message on misconfiguration
- Prevents accidental use of default credentials
- Requires all DB_* environment variables to be explicitly set

### Credential Rotation

When rotating credentials:
1. Update environment variables in your deployment system
2. Restart the application to pick up new credentials
3. The application will validate the new connection on startup

## Qdrant Security

The application automatically uses Qdrant API key authentication if configured:

1. **Set the API key in `.env`:**
   ```bash
   QDRANT_API_KEY=your_secure_api_key_here
   ```

2. **The application will automatically:**
   - Detect the API key from environment
   - Use it for all Qdrant connections
   - Work without authentication if not set (for development)

3. **Generate a secure API key:**
   ```bash
   # Generate a cryptographically secure key
   openssl rand -hex 32
   ```

## Additional Security Considerations

- **Path Traversal Protection**: Built-in path validation prevents directory traversal attacks
- **Error Sanitization**: Sensitive information is sanitized from error messages
- **Input Validation**: All user inputs are validated before processing
- **Automatic Authentication**: Qdrant API key is automatically applied when set

## Reporting Security Issues

If you discover a security vulnerability, please report it to:
- Open a private security advisory on GitHub
- Do not disclose publicly until a fix is available