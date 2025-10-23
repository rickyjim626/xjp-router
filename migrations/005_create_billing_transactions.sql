-- Create billing_transactions table for per-request cost tracking
-- Migration: 005
-- Description: Real-time billing system with API key-based cost attribution

CREATE TABLE billing_transactions (
    -- Primary key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Tenant & API Key
    tenant_id VARCHAR(255) NOT NULL,
    api_key_id UUID NOT NULL REFERENCES api_keys(id) ON DELETE CASCADE,

    -- Request metadata
    request_id VARCHAR(255) NOT NULL UNIQUE,
    logical_model VARCHAR(100) NOT NULL,
    provider VARCHAR(50) NOT NULL,
    provider_model_id VARCHAR(255) NOT NULL,

    -- Token usage
    prompt_tokens BIGINT NOT NULL DEFAULT 0,
    completion_tokens BIGINT NOT NULL DEFAULT 0,
    reasoning_tokens BIGINT NOT NULL DEFAULT 0,
    cached_prompt_tokens BIGINT NOT NULL DEFAULT 0,
    total_tokens BIGINT NOT NULL DEFAULT 0,

    -- Cost breakdown (USD)
    prompt_cost DECIMAL(12, 8) NOT NULL DEFAULT 0,
    completion_cost DECIMAL(12, 8) NOT NULL DEFAULT 0,
    reasoning_cost DECIMAL(12, 8) NOT NULL DEFAULT 0,
    cache_read_cost DECIMAL(12, 8) NOT NULL DEFAULT 0,
    request_cost DECIMAL(12, 8) NOT NULL DEFAULT 0,
    total_cost DECIMAL(12, 8) NOT NULL DEFAULT 0,

    -- Pricing snapshot (for audit)
    pricing_snapshot JSONB NOT NULL,

    -- Response metadata
    response_time_ms INTEGER,
    status VARCHAR(20) NOT NULL,
    error_message TEXT,

    -- Timestamp
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT positive_tokens CHECK (total_tokens >= 0),
    CONSTRAINT positive_cost CHECK (total_cost >= 0),
    CONSTRAINT valid_status CHECK (status IN ('success', 'error', 'timeout'))
);

-- Indexes for efficient queries
CREATE INDEX idx_billing_tenant_time ON billing_transactions(tenant_id, created_at DESC);
CREATE INDEX idx_billing_api_key_time ON billing_transactions(api_key_id, created_at DESC);
CREATE INDEX idx_billing_created_at ON billing_transactions(created_at DESC);
CREATE INDEX idx_billing_request_id ON billing_transactions(request_id);
CREATE INDEX idx_billing_status ON billing_transactions(status);
CREATE INDEX idx_billing_provider ON billing_transactions(provider, created_at DESC);
CREATE INDEX idx_billing_logical_model ON billing_transactions(logical_model, created_at DESC);

-- Comment
COMMENT ON TABLE billing_transactions IS 'Real-time billing transactions per API request';
COMMENT ON COLUMN billing_transactions.request_id IS 'Unique request ID for idempotency';
COMMENT ON COLUMN billing_transactions.pricing_snapshot IS 'ModelPricing at request time for audit';
COMMENT ON COLUMN billing_transactions.cached_prompt_tokens IS 'Prompt cache hits (Claude/OpenAI)';
COMMENT ON COLUMN billing_transactions.reasoning_tokens IS 'Thinking/reasoning tokens (o1/Claude Extended Thinking)';
