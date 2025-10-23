#!/bin/bash
# Initialize secrets in xjp-secret-store
# Usage: ./scripts/init-secrets.sh

set -e

# Configuration
BASE_URL="${SECRET_STORE_BASE_URL:-https://kskxndnvmqwr.sg-members-1.clawcloudrun.com}"
API_KEY="${SECRET_STORE_API_KEY:-xjp_jIBtHdAv5cVfEi5OX77fwGGk1SKl4UAQdTXIHkhS}"
NAMESPACE="router"

echo "🔐 Initializing secrets in xjp-secret-store"
echo "   Base URL: $BASE_URL"
echo "   Namespace: $NAMESPACE"
echo ""

# Function to put a secret
put_secret() {
    local key="$1"
    local value="$2"
    local description="${3:-No description}"

    echo "  ⏳ Storing: $key"

    response=$(curl -s -w "\n%{http_code}" -X PUT "$BASE_URL/api/v2/secrets/$NAMESPACE/$key" \
        -H "x-api-key: $API_KEY" \
        -H "Content-Type: application/json" \
        -d "{\"value\": \"$value\"}")

    http_code=$(echo "$response" | tail -n1)
    body=$(echo "$response" | head -n-1)

    if [ "$http_code" -eq 200 ] || [ "$http_code" -eq 201 ]; then
        echo "  ✅ Stored: $key"
    else
        echo "  ❌ Failed to store $key (HTTP $http_code)"
        echo "     Response: $body"
    fi
}

# Check required environment variables
if [ -z "$OPENROUTER_API_KEY" ]; then
    echo "⚠️  Warning: OPENROUTER_API_KEY not set"
fi

if [ -z "$VERTEX_API_KEY" ] && [ -z "$VERTEX_ACCESS_TOKEN" ]; then
    echo "⚠️  Warning: Neither VERTEX_API_KEY nor VERTEX_ACCESS_TOKEN set"
fi

if [ -z "$DATABASE_URL" ]; then
    echo "⚠️  Warning: DATABASE_URL not set"
fi

echo ""
echo "📦 Storing secrets..."
echo ""

# Store OpenRouter secrets
if [ -n "$OPENROUTER_API_KEY" ]; then
    put_secret "providers/openrouter/api-key" "$OPENROUTER_API_KEY" "OpenRouter API key"
fi

# Store Vertex AI secrets
if [ -n "$VERTEX_API_KEY" ]; then
    put_secret "providers/vertex/api-key" "$VERTEX_API_KEY" "Vertex AI API key"
fi

if [ -n "$VERTEX_ACCESS_TOKEN" ]; then
    put_secret "providers/vertex/access-token" "$VERTEX_ACCESS_TOKEN" "Vertex AI access token"
fi

if [ -n "$VERTEX_PROJECT" ]; then
    put_secret "providers/vertex/project" "$VERTEX_PROJECT" "Vertex AI project ID"
fi

if [ -n "$VERTEX_REGION" ]; then
    put_secret "providers/vertex/region" "$VERTEX_REGION" "Vertex AI region"
fi

# Store Clewdr secrets
if [ -n "$CLEWDR_API_KEY" ]; then
    put_secret "providers/clewdr/api-key" "$CLEWDR_API_KEY" "Clewdr API key"
fi

if [ -n "$CLEWDR_BASE_URL" ]; then
    put_secret "providers/clewdr/base-url" "$CLEWDR_BASE_URL" "Clewdr base URL"
fi

# Store infrastructure secrets
if [ -n "$DATABASE_URL" ]; then
    put_secret "infrastructure/database-url" "$DATABASE_URL" "PostgreSQL database URL"
fi

echo ""
echo "✅ Secret initialization complete!"
echo ""
echo "To verify, run:"
echo "  cargo run --bin export_secrets -- --namespace $NAMESPACE"
