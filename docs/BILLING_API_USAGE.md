# Billing API Usage Guide

## Overview

The billing API provides cost estimation and verification for OpenRouter and Vertex AI model usage. It supports dynamic price fetching from OpenRouter and detailed cost breakdown calculation.

## API Endpoint

```
POST /internal/billing/quote
```

## Prerequisites

Set the OpenRouter API key in your environment:
```bash
export OPENROUTER_API_KEY=sk-or-v1-********************************
```

## Use Cases

### 1. Query Pricing Only

Get the latest pricing for a specific model without usage data:

```bash
curl -X POST http://localhost:8080/internal/billing/quote \
  -H "Content-Type: application/json" \
  -d '{
    "provider_model_id": "anthropic/claude-4.5-sonnet"
  }'
```

**Response:**
```json
{
  "pricing_only": {
    "prompt": 0.000003,
    "completion": 0.000015,
    "request": 0.0,
    "image": 0.0,
    "internal_reasoning": 0.0,
    "input_cache_read": 0.0000003,
    "input_cache_write": 0.00000375
  }
}
```

### 2. Calculate Cost with Usage

Verify billing by providing usage data (from OpenRouter response):

```bash
curl -X POST http://localhost:8080/internal/billing/quote \
  -H "Content-Type: application/json" \
  -d '{
    "provider_model_id": "anthropic/claude-4.5-sonnet",
    "usage": {
      "usage": {
        "prompt_tokens": 1800,
        "prompt_tokens_details": {
          "cached_tokens": 600
        },
        "completion_tokens": 320,
        "completion_tokens_details": {
          "reasoning_tokens": 24
        }
      }
    }
  }'
```

**Response:**
```json
{
  "pricing": {
    "prompt": 0.000003,
    "completion": 0.000015,
    "request": 0.0,
    "image": 0.0,
    "internal_reasoning": 0.0,
    "input_cache_read": 0.0000003,
    "input_cache_write": 0.00000375
  },
  "usage": {
    "prompt_tokens": 1800,
    "completion_tokens": 320,
    "reasoning_tokens": 24,
    "cached_prompt_tokens": 600
  },
  "breakdown": {
    "prompt_tokens": 1800,
    "completion_tokens": 320,
    "reasoning_tokens": 24,
    "cached_prompt_tokens": 600,
    "prompt_cost": 0.0036,
    "completion_cost": 0.0048,
    "internal_reasoning_cost": 0.00036,
    "cache_read_cost": 0.00018,
    "request_cost": 0.0,
    "total_cost": 0.00894,
    "unit": "USD"
  }
}
```

## Supported Models

### Claude Models
- `anthropic/claude-4.5-sonnet`
- `anthropic/claude-4.1-opus`
- `anthropic/claude-3-opus`
- `anthropic/claude-3-sonnet`
- `anthropic/claude-3-haiku`

### Gemini Models
- `google/gemini-2.5-pro`
- `google/gemini-2.5-pro-002`
- `google/gemini-2.5-flash`
- `google/gemini-2.5-flash-002`

### GPT Models
- `openai/gpt-5-*`
- `openai/gpt-4o`
- `openai/gpt-4-turbo`

*Note: Full model list available at `https://openrouter.ai/api/v1/models`*

## Cost Breakdown Explanation

| Field | Description |
|-------|-------------|
| `prompt_cost` | Cost for non-cached prompt tokens |
| `cache_read_cost` | Cost for reading cached prompt tokens (0.1x for Claude, 0.25x-0.5x for OpenAI) |
| `completion_cost` | Cost for regular completion tokens |
| `internal_reasoning_cost` | Cost for reasoning/thinking tokens (usually same as completion rate) |
| `request_cost` | Fixed per-request cost (if applicable) |
| `total_cost` | Sum of all above costs in USD |

## Verifying OpenRouter Bills

1. Make a request through the gateway:
```bash
curl -X POST http://localhost:8080/v1/chat/completions \
  -H "Authorization: Bearer XJP_your_key" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4.5",
    "messages": [{"role": "user", "content": "Hello"}],
    "stream": false
  }' | tee response.json
```

2. Extract the `usage` field from the response and verify costs:
```bash
# Extract usage from response.json
USAGE=$(jq '.usage' response.json)

# Query billing API
curl -X POST http://localhost:8080/internal/billing/quote \
  -H "Content-Type: application/json" \
  -d "{
    \"provider_model_id\": \"anthropic/claude-4.5-sonnet\",
    \"usage\": {\"usage\": $USAGE}
  }"
```

The `breakdown.total_cost` should match OpenRouter's `usage.cost` field (within rounding precision).

## Error Handling

### Missing API Key
```json
{
  "error": "OPENROUTER_API_KEY not set for pricing fetch"
}
```

**Solution:** Set the environment variable before starting the gateway.

### Model Not Found
```json
{
  "error": "pricing not found for model my-invalid-model"
}
```

**Solution:** Check the model ID against OpenRouter's model list.

### Invalid Usage Format
The API expects usage in OpenRouter format. Ensure the structure matches:
```json
{
  "usage": {
    "prompt_tokens": 123,
    "completion_tokens": 45,
    "prompt_tokens_details": {
      "cached_tokens": 67
    },
    "completion_tokens_details": {
      "reasoning_tokens": 8
    }
  }
}
```

## Caching

- Price cache TTL: **15 minutes**
- Prices are automatically refreshed when the cache expires
- All requests share the same cache to minimize OpenRouter API calls

## Integration Tips

1. **Real-time Billing**: Call this API after each completion to log costs to your database
2. **Budget Monitoring**: Query pricing before making expensive requests
3. **Cost Attribution**: Use with different XJP keys to track per-tenant costs
4. **Audit Trail**: Compare calculated costs with OpenRouter invoices monthly
