#!/bin/bash
# C3 VM 自动化部署和测试脚本
# 使用方法: bash c3_auto_test.sh

set -e  # 遇到错误立即退出

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# 检查命令是否存在
check_command() {
    if ! command -v $1 &> /dev/null; then
        log_error "$1 未安装"
        return 1
    fi
    log_info "$1 已安装 ✓"
    return 0
}

# ============================================
# Step 0: 前置检查
# ============================================
log_info "=== Step 0: 检查环境 ==="

check_command git || exit 1
check_command cargo || exit 1
check_command psql || exit 1
check_command curl || exit 1

# ============================================
# Step 1: 拉取代码
# ============================================
log_info "=== Step 1: 拉取最新代码 ==="

# 获取当前分支
CURRENT_BRANCH=$(git branch --show-current)
log_info "当前分支: $CURRENT_BRANCH"

# 显示当前 commit
BEFORE_COMMIT=$(git rev-parse --short HEAD)
log_info "拉取前 commit: $BEFORE_COMMIT"

# 拉取代码
git pull origin $CURRENT_BRANCH

# 显示新 commit
AFTER_COMMIT=$(git rev-parse --short HEAD)
log_info "拉取后 commit: $AFTER_COMMIT"

if [ "$BEFORE_COMMIT" = "$AFTER_COMMIT" ]; then
    log_info "代码已是最新"
else
    log_info "代码已更新: $BEFORE_COMMIT -> $AFTER_COMMIT"
fi

# 显示最新 commit 信息
log_info "最新 commit:"
git log -1 --oneline

# ============================================
# Step 2: 配置环境变量
# ============================================
log_info "=== Step 2: 检查环境变量 ==="

# 检查 DATABASE_URL
if [ -z "$DATABASE_URL" ]; then
    log_warn "DATABASE_URL 未设置"
    read -p "请输入 DATABASE_URL (例: postgres://user:pass@localhost:5432/xjp_gateway): " DATABASE_URL
    export DATABASE_URL
else
    log_info "DATABASE_URL 已设置: ${DATABASE_URL:0:30}..."
fi

# 检查 OPENROUTER_API_KEY
if [ -z "$OPENROUTER_API_KEY" ]; then
    log_warn "OPENROUTER_API_KEY 未设置"
    read -p "请输入 OPENROUTER_API_KEY: " OPENROUTER_API_KEY
    export OPENROUTER_API_KEY
else
    log_info "OPENROUTER_API_KEY 已设置: ${OPENROUTER_API_KEY:0:15}..."
fi

# 测试数据库连接
log_info "测试数据库连接..."
if psql "$DATABASE_URL" -c "SELECT version();" &>/dev/null; then
    log_info "数据库连接成功 ✓"
else
    log_error "数据库连接失败"
    exit 1
fi

# ============================================
# Step 3: 安装 sqlx-cli
# ============================================
log_info "=== Step 3: 检查 sqlx-cli ==="

if ! command -v sqlx &> /dev/null; then
    log_warn "sqlx-cli 未安装，正在安装..."
    cargo install sqlx-cli --no-default-features --features postgres
else
    log_info "sqlx-cli 已安装 ✓"
fi

# ============================================
# Step 4: 运行数据库迁移
# ============================================
log_info "=== Step 4: 运行数据库迁移 ==="

log_info "当前迁移状态:"
sqlx migrate info || true

log_info "运行迁移..."
sqlx migrate run

log_info "迁移完成，检查表是否存在:"
psql "$DATABASE_URL" -c "\dt" | grep -E "billing_transactions|tenant_billing_summary" && \
    log_info "计费表创建成功 ✓" || \
    log_warn "计费表未找到"

# ============================================
# Step 5: 生成 SQLx 离线数据
# ============================================
log_info "=== Step 5: 生成 SQLx 离线数据 ==="

log_info "正在生成 SQLx 缓存（这可能需要几分钟）..."
cargo sqlx prepare

if [ -d ".sqlx" ]; then
    log_info "SQLx 离线数据生成成功 ✓"
    ls -lh .sqlx/ | head -5
else
    log_error "SQLx 离线数据生成失败"
    exit 1
fi

# ============================================
# Step 6: 构建项目
# ============================================
log_info "=== Step 6: 构建项目 ==="

log_info "开始构建（Release 模式）..."
START_TIME=$(date +%s)

if cargo build --release 2>&1 | tee build.log; then
    END_TIME=$(date +%s)
    BUILD_TIME=$((END_TIME - START_TIME))
    log_info "构建成功 ✓ (耗时: ${BUILD_TIME}秒)"

    # 显示二进制文件大小
    BINARY_SIZE=$(du -h target/release/xjp-gateway | cut -f1)
    log_info "二进制文件大小: $BINARY_SIZE"
else
    log_error "构建失败，请查看 build.log"
    exit 1
fi

# ============================================
# Step 7: 启动服务
# ============================================
log_info "=== Step 7: 启动服务 ==="

# 检查端口是否被占用
PORT=${PORT:-8080}
if lsof -i :$PORT &>/dev/null; then
    log_warn "端口 $PORT 已被占用，尝试停止旧进程..."
    pkill -f xjp-gateway || true
    sleep 2
fi

# 启动服务（后台）
log_info "启动服务（端口: $PORT）..."
RUST_LOG=info,xjp_gateway=debug \
    nohup ./target/release/xjp-gateway > gateway.log 2>&1 &

SERVICE_PID=$!
log_info "服务已启动 (PID: $SERVICE_PID)"

# 等待服务启动
log_info "等待服务启动..."
for i in {1..30}; do
    if curl -s http://localhost:$PORT/healthz &>/dev/null; then
        log_info "服务启动成功 ✓"
        break
    fi
    if [ $i -eq 30 ]; then
        log_error "服务启动超时"
        tail -20 gateway.log
        exit 1
    fi
    sleep 1
done

# ============================================
# Step 8: 运行测试
# ============================================
log_info "=== Step 8: 运行功能测试 ==="

# 测试 1: 健康检查
log_info "测试 1: 健康检查..."
HEALTH_RESPONSE=$(curl -s http://localhost:$PORT/healthz)
if [ "$HEALTH_RESPONSE" = "ok" ]; then
    log_info "健康检查通过 ✓"
else
    log_error "健康检查失败: $HEALTH_RESPONSE"
fi

# 测试 2: Metrics 端点
log_info "测试 2: Metrics 端点..."
if curl -s http://localhost:$PORT/metrics | grep -q "xjp_"; then
    log_info "Metrics 端点正常 ✓"
else
    log_warn "Metrics 端点异常"
fi

# 测试 3: 价格查询（Phase 1）
log_info "测试 3: 价格查询 API..."
PRICE_RESPONSE=$(curl -s http://localhost:$PORT/internal/billing/quote \
    -H "Content-Type: application/json" \
    -d '{
        "provider_model_id": "anthropic/claude-4.5-sonnet"
    }')

if echo "$PRICE_RESPONSE" | jq -e '.pricing_only.prompt' &>/dev/null; then
    PROMPT_PRICE=$(echo "$PRICE_RESPONSE" | jq -r '.pricing_only.prompt')
    log_info "价格查询成功 ✓ (prompt: $PROMPT_PRICE USD/token)"
else
    log_error "价格查询失败: $PRICE_RESPONSE"
fi

# 测试 4: 成本计算
log_info "测试 4: 成本计算 API..."
COST_RESPONSE=$(curl -s http://localhost:$PORT/internal/billing/quote \
    -H "Content-Type: application/json" \
    -d '{
        "provider_model_id": "anthropic/claude-4.5-sonnet",
        "usage": {
            "usage": {
                "prompt_tokens": 1000,
                "completion_tokens": 500,
                "prompt_tokens_details": {"cached_tokens": 200},
                "completion_tokens_details": {"reasoning_tokens": 50}
            }
        }
    }')

if echo "$COST_RESPONSE" | jq -e '.breakdown.total_cost' &>/dev/null; then
    TOTAL_COST=$(echo "$COST_RESPONSE" | jq -r '.breakdown.total_cost')
    log_info "成本计算成功 ✓ (total: $TOTAL_COST USD)"

    # 显示详细明细
    log_info "成本明细:"
    echo "$COST_RESPONSE" | jq '.breakdown' | sed 's/^/    /'
else
    log_error "成本计算失败: $COST_RESPONSE"
fi

# 测试 5: 数据库计费表
log_info "测试 5: 检查计费表..."
TRANSACTION_COUNT=$(psql "$DATABASE_URL" -t -c "SELECT COUNT(*) FROM billing_transactions;" | xargs)
log_info "当前计费记录数: $TRANSACTION_COUNT"

# 测试 6: 计费查询 API（需要有数据）
if [ "$TRANSACTION_COUNT" -gt 0 ]; then
    log_info "测试 6: 计费历史查询..."

    # 获取一个 tenant_id
    TENANT_ID=$(psql "$DATABASE_URL" -t -c "SELECT tenant_id FROM billing_transactions LIMIT 1;" | xargs)

    if [ -n "$TENANT_ID" ]; then
        TRANSACTIONS_RESPONSE=$(curl -s "http://localhost:$PORT/internal/billing/transactions?tenant_id=$TENANT_ID&limit=5")

        if echo "$TRANSACTIONS_RESPONSE" | jq -e '.transactions' &>/dev/null; then
            TRANS_COUNT=$(echo "$TRANSACTIONS_RESPONSE" | jq '.transactions | length')
            log_info "历史查询成功 ✓ (返回 $TRANS_COUNT 条记录)"
        else
            log_warn "历史查询失败"
        fi
    fi
else
    log_warn "跳过历史查询测试（无计费数据）"
fi

# ============================================
# Step 9: 显示测试结果
# ============================================
log_info "=== Step 9: 测试总结 ==="

echo ""
echo "========================================="
echo "  部署和测试完成"
echo "========================================="
echo ""
echo "服务信息:"
echo "  - PID: $SERVICE_PID"
echo "  - 端口: $PORT"
echo "  - 日志: $(pwd)/gateway.log"
echo ""
echo "可用端点:"
echo "  - 健康检查: http://localhost:$PORT/healthz"
echo "  - Metrics: http://localhost:$PORT/metrics"
echo "  - 价格查询: POST http://localhost:$PORT/internal/billing/quote"
echo "  - 历史查询: GET http://localhost:$PORT/internal/billing/transactions"
echo "  - 成本汇总: GET http://localhost:$PORT/internal/billing/summary"
echo ""
echo "查看日志:"
echo "  tail -f gateway.log"
echo ""
echo "停止服务:"
echo "  kill $SERVICE_PID"
echo ""

# 保存 PID 到文件
echo $SERVICE_PID > .xjp-gateway.pid
log_info "PID 已保存到 .xjp-gateway.pid"

echo "========================================="
echo "  ${GREEN}✓ 部署成功！${NC}"
echo "========================================="
