# Migration Guide: Environment Variables → xjp-secret-store

This guide walks you through migrating your xjp-router deployment from environment variables to centralized secret management with xjp-secret-store.

## Prerequisites

- Access to xjp-secret-store API
- API key with `router:admin` permissions
- Current environment variables documented

## Step 1: Backup Current Configuration

```bash
# Export current environment variables
env | grep -E "(OPENROUTER|VERTEX|CLEWDR|DATABASE)" > .env.backup

# Create a backup of your config
cp config/xjp.toml config/xjp.toml.backup
```

## Step 2: Initialize Secrets in Secret Store

```bash
# Set your secret store credentials
export SECRET_STORE_API_KEY="xjp_jIBtHdAv5cVfEi5OX77fwGGk1SKl4UAQdTXIHkhS"

# Run the initialization script
./scripts/init-secrets.sh
```

This will upload all your existing environment variables to the secret store under the `router` namespace.

## Step 3: Verify Secrets

```bash
# List all secrets in the router namespace
cargo run --bin export_secrets -- --namespace router --format dotenv
```

Compare the output with your `.env.backup` to ensure all secrets were uploaded correctly.

## Step 4: Enable Secret Store in Configuration

Edit `config/xjp.toml`:

```toml
[secret_store]
enabled = true  # Change from false to true
base_url = "https://kskxndnvmqwr.sg-members-1.clawcloudrun.com"
namespace = "router"
cache_ttl_secs = 300
retries = 3
timeout_ms = 10000
preload = true
preload_keys = [
    "providers/openrouter/api-key",
    "providers/vertex/api-key",
    "providers/vertex/access-token",
    "providers/vertex/project",
    "providers/vertex/region",
    "providers/clewdr/api-key",
    "infrastructure/database-url"
]
```

## Step 5: Test with Environment Variable Fallback

Deploy with **both** secret store enabled **and** environment variables present. This provides a safety net:

```bash
# Keep your existing environment variables
source .env.backup

# Add secret store API key
export SECRET_STORE_API_KEY="xjp_jIBtHdAv5cVfEi5OX77fwGGk1SKl4UAQdTXIHkhS"

# Run the gateway
cargo run --release
```

Check the logs for:
- `"Secret store enabled, connecting to..."`
- `"SDK provider initialized successfully"`
- `"Successfully preloaded X secrets"`

## Step 6: Monitor for 48 Hours

Monitor your deployment for issues:

```bash
# Check logs for secret fetch errors
kubectl logs -f deployment/xjp-router | grep -i "secret"

# Monitor metrics
curl http://localhost:8080/metrics | grep secret_store
```

Expected metrics:
- `secret_store_requests_total` - Should be > 0
- `secret_store_cache_hits_total` - Should grow over time
- `secret_store_errors_total` - Should be 0

## Step 7: Remove Environment Variables (Optional)

Once you're confident the integration is working, you can remove the environment variable fallbacks:

```bash
# Remove provider secrets from environment
unset OPENROUTER_API_KEY
unset VERTEX_API_KEY
unset VERTEX_ACCESS_TOKEN
unset VERTEX_PROJECT
unset VERTEX_REGION
unset CLEWDR_API_KEY
unset DATABASE_URL

# Keep only the secret store API key
export SECRET_STORE_API_KEY="xjp_jIBtHdAv5cVfEi5OX77fwGGk1SKl4UAQdTXIHkhS"

# Restart
cargo run --release
```

## Step 8: Update Deployment Configurations

Update your deployment manifests (Docker Compose, Kubernetes, etc.):

### Docker Compose

```yaml
services:
  xjp-router:
    environment:
      - SECRET_STORE_API_KEY=${SECRET_STORE_API_KEY}
      # Remove all other secrets
    # ...
```

### Kubernetes

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: xjp-router-secrets
stringData:
  SECRET_STORE_API_KEY: "xjp_jIBtHdAv5cVfEi5OX77fwGGk1SKl4UAQdTXIHkhS"
---
apiVersion: apps/v1
kind: Deployment
spec:
  template:
    spec:
      containers:
      - name: xjp-router
        env:
        - name: SECRET_STORE_API_KEY
          valueFrom:
            secretKeyRef:
              name: xjp-router-secrets
              key: SECRET_STORE_API_KEY
        # Remove all other environment variables
```

## Rollback Procedure

If you encounter issues, you can quickly rollback:

1. **Disable secret store** in `config/xjp.toml`:
   ```toml
   [secret_store]
   enabled = false
   ```

2. **Restore environment variables**:
   ```bash
   source .env.backup
   ```

3. **Restart the service**

## Secret Rotation

To rotate a secret:

```bash
# Update the secret in secret-store
curl -X PUT "https://kskxndnvmqwr.sg-members-1.clawcloudrun.com/api/v2/secrets/router/providers/openrouter/api-key" \
  -H "x-api-key: $SECRET_STORE_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"value": "new-api-key-value"}'

# Wait for cache TTL (default: 5 minutes) or restart the service
# The service will automatically fetch the new value on next cache miss
```

## Troubleshooting

### Error: "Failed to initialize SDK provider"

- Verify `SECRET_STORE_API_KEY` is correct
- Check network connectivity to secret store
- Verify API key has proper permissions (`router:admin`)

### Error: "Secret not found"

- Verify secrets were uploaded: `cargo run --bin export_secrets`
- Check secret key names match exactly
- Ensure namespace is correct (`router`)

### Performance Issues

- Check cache hit rate in logs
- Increase `cache_ttl_secs` if appropriate
- Reduce `preload_keys` list if too large

## Benefits After Migration

✅ **Centralized Management**: All secrets in one place
✅ **Audit Trail**: Track who accesses what and when
✅ **Easy Rotation**: Update secrets without redeploying
✅ **Version History**: Rollback to previous secret values
✅ **Reduced Risk**: Fewer places secrets can leak
✅ **Compliance**: Meet security compliance requirements

## Support

For issues or questions:
- Check logs: `kubectl logs -f deployment/xjp-router`
- Review metrics: `curl http://localhost:8080/metrics`
- Open an issue: [GitHub Repository]
