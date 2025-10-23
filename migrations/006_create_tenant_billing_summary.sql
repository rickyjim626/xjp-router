-- Create tenant_billing_summary table for aggregated billing data
-- Migration: 006
-- Description: Aggregated billing summaries for fast reporting

CREATE TABLE tenant_billing_summary (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Tenant & API Key
    tenant_id VARCHAR(255) NOT NULL,
    api_key_id UUID NOT NULL REFERENCES api_keys(id) ON DELETE CASCADE,

    -- Time period
    period_start TIMESTAMPTZ NOT NULL,
    period_end TIMESTAMPTZ NOT NULL,
    period_type VARCHAR(20) NOT NULL,

    -- Statistics
    total_requests BIGINT NOT NULL DEFAULT 0,
    successful_requests BIGINT NOT NULL DEFAULT 0,
    failed_requests BIGINT NOT NULL DEFAULT 0,

    total_tokens BIGINT NOT NULL DEFAULT 0,
    total_cost DECIMAL(12, 8) NOT NULL DEFAULT 0,

    -- Model breakdown (JSONB for flexibility)
    -- Format: {"claude-sonnet-4.5": {"requests": 100, "tokens": 50000, "cost": 1.23}, ...}
    model_breakdown JSONB NOT NULL DEFAULT '{}',

    -- Provider breakdown
    -- Format: {"OpenRouter": {"requests": 80, "cost": 0.98}, "Vertex": {"requests": 20, "cost": 0.25}}
    provider_breakdown JSONB NOT NULL DEFAULT '{}',

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT valid_period_type CHECK (period_type IN ('daily', 'weekly', 'monthly')),
    CONSTRAINT positive_requests CHECK (total_requests >= 0),
    CONSTRAINT positive_cost_summary CHECK (total_cost >= 0),
    CONSTRAINT valid_period CHECK (period_end > period_start),
    UNIQUE(tenant_id, api_key_id, period_type, period_start)
);

-- Indexes
CREATE INDEX idx_summary_tenant_period ON tenant_billing_summary(tenant_id, period_type, period_start DESC);
CREATE INDEX idx_summary_api_key_period ON tenant_billing_summary(api_key_id, period_type, period_start DESC);
CREATE INDEX idx_summary_period_start ON tenant_billing_summary(period_start DESC);

-- Function to update summary
CREATE OR REPLACE FUNCTION update_billing_summary_timestamp()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger to auto-update updated_at
CREATE TRIGGER trigger_update_summary_timestamp
    BEFORE UPDATE ON tenant_billing_summary
    FOR EACH ROW
    EXECUTE FUNCTION update_billing_summary_timestamp();

-- Comment
COMMENT ON TABLE tenant_billing_summary IS 'Aggregated billing data for fast reporting queries';
COMMENT ON COLUMN tenant_billing_summary.period_type IS 'Aggregation period: daily, weekly, monthly';
COMMENT ON COLUMN tenant_billing_summary.model_breakdown IS 'Per-model usage statistics (JSONB)';
COMMENT ON COLUMN tenant_billing_summary.provider_breakdown IS 'Per-provider usage statistics (JSONB)';
