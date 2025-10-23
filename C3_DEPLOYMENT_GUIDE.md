# C3 VM 部署与测试指南

## 📋 前置条件

- C3 VM 访问权限
- PostgreSQL 15+ (应该已安装)
- Rust 工具链 (应该已安装)
- Git (应该已安装)

---

## 🚀 快速部署流程

### Step 1: 拉取最新代码

```bash
# SSH 到 C3 VM
ssh user@c3-vm-address

# 进入项目目录
cd /path/to/xjp-router  # 替换为实际路径

# 拉取最新代码
git pull origin master

# 验证最新 commit
git log --oneline -1
# 应该看到: feat: Complete billing system with real-time cost tracking
```

### Step 2: 配置数据库

#### 选项 A: 使用现有数据库（推荐）

```bash
# 检查数据库是否运行
psql -U postgres -c "SELECT version();"

# 如果数据库已存在，只需设置环境变量
export DATABASE_URL=postgres://postgres:password@localhost:5432/xjp_gateway

# 或者从现有配置获取
# export DATABASE_URL=$(grep DATABASE_URL .env | cut -d '=' -f2)
```

#### 选项 B: 创建新数据库

```bash
# 创建数据库（如果不存在）
psql -U postgres -c "CREATE DATABASE xjp_gateway;"

# 设置环境变量
export DATABASE_URL=postgres://postgres:your_password@localhost:5432/xjp_gateway
```

### Step 3: 运行数据库迁移

```bash
# 安装 sqlx-cli（如果未安装）
cargo install sqlx-cli --no-default-features --features postgres

# 运行所有迁移（包括新的计费表）
sqlx migrate run

# 验证迁移成功
psql $DATABASE_URL -c "\dt"
# 应该看到: billing_transactions, tenant_billing_summary
```

### Step 4: 配置环境变量

```bash
# 创建或编辑 .env 文件
cat > .env <<'EOF'
# Database
DATABASE_URL=postgres://postgres:password@localhost:5432/xjp_gateway

# OpenRouter (必需 - 用于价格拉取和出站)
OPENROUTER_API_KEY=sk-or-v1-********************************

# Vertex AI (可选)
VERTEX_API_KEY=AIza***********************************
VERTEX_PROJECT=your-gcp-project
VERTEX_REGION=us-central1

# Clewdr (可选)
CLEWDR_BASE_URL=http://localhost:9000
CLEWDR_API_KEY=optional

# Secret Store (可选)
SECRET_STORE_API_KEY=your-secret-store-key

# Logging
RUST_LOG=info,xjp_gateway=debug

# Server
PORT=8080
EOF

# 加载环境变量
source .env
```

### Step 5: 生成 SQLx 离线数据

```bash
# 这一步会连接数据库验证所有 SQL 查询
cargo sqlx prepare

# 验证生成的文件
ls -la .sqlx/
# 应该看到: query-*.json 文件
```

### Step 6: 构建项目

```bash
# 完整构建（Release 模式）
cargo build --release

# 或者 Debug 模式（更快）
cargo build

# 验证构建成功
./target/release/xjp-gateway --version
```

### Step 7: 运行测试

```bash
# 运行单元测试
cargo test

# 启动服务（后台运行）
nohup ./target/release/xjp-gateway > gateway.log 2>&1 &

# 或者前台运行（方便调试）
RUST_LOG=debug ./target/release/xjp-gateway
```

---

## ✅ 验证部署

### 1. 健康检查

```bash
curl http://localhost:8080/healthz
# 预期: "ok"
```

### 2. 查询价格（Phase 1）

```bash
curl -X POST http://localhost:8080/internal/billing/quote \
  -H "Content-Type: application/json" \
  -d '{
    "provider_model_id": "anthropic/claude-4.5-sonnet"
  }'

# 预期: 返回 pricing_only 对象
```

### 3. 测试完整计费流程（Phase 2）

```bash
# 先创建测试 API Key（如果还没有）
# psql $DATABASE_URL -c "INSERT INTO api_keys ..."

# 发起实际请求
curl -X POST http://localhost:8080/v1/chat/completions \
  -H "Authorization: Bearer XJP_your_test_key" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4.5",
    "messages": [{"role": "user", "content": "Hello, test billing!"}],
    "stream": false
  }'

# 查询计费记录（等待 1-2 秒让异步写入完成）
sleep 2
psql $DATABASE_URL -c "SELECT id, tenant_id, logical_model, total_tokens, total_cost, status FROM billing_transactions ORDER BY created_at DESC LIMIT 1;"
```

### 4. 查询计费历史

```bash
# 按租户查询
curl "http://localhost:8080/internal/billing/transactions?tenant_id=your-tenant-id&limit=10"

# 查询成本汇总（本月）
START=$(date -u +"%Y-%m-01T00:00:00Z")
END=$(date -u +"%Y-%m-%dT23:59:59Z")
curl "http://localhost:8080/internal/billing/summary?tenant_id=your-tenant-id&start=$START&end=$END"
```

---

## 🔧 故障排查

### 问题 1: 编译失败 - sqlx 错误

**症状**:
```
error: error communicating with database: Connection refused
```

**解决**:
```bash
# 检查数据库是否运行
systemctl status postgresql  # 或 service postgresql status

# 检查 DATABASE_URL
echo $DATABASE_URL

# 测试连接
psql $DATABASE_URL -c "SELECT 1;"

# 重新生成 SQLx 数据
cargo sqlx prepare --check  # 先检查
cargo sqlx prepare          # 重新生成
```

### 问题 2: 迁移失败

**症状**:
```
ERROR: relation "billing_transactions" already exists
```

**解决**:
```bash
# 检查当前迁移状态
sqlx migrate info

# 如果需要重置（小心！会删除数据）
sqlx migrate revert  # 回退最后一个迁移
sqlx migrate run     # 重新运行
```

### 问题 3: 价格拉取失败

**症状**:
```
{"error": "OPENROUTER_API_KEY not set for pricing fetch"}
```

**解决**:
```bash
# 验证环境变量
echo $OPENROUTER_API_KEY

# 测试 API Key
curl -H "Authorization: Bearer $OPENROUTER_API_KEY" \
  https://openrouter.ai/api/v1/models | jq '.data[0]'
```

### 问题 4: 计费记录未写入

**症状**: 请求成功但数据库无记录

**调试**:
```bash
# 查看日志（寻找 "Failed to insert billing transaction"）
tail -f gateway.log | grep billing

# 检查数据库连接池
psql $DATABASE_URL -c "SELECT count(*) FROM pg_stat_activity;"

# 手动测试插入
psql $DATABASE_URL -c "INSERT INTO billing_transactions (id, tenant_id, api_key_id, request_id, logical_model, provider, provider_model_id, prompt_tokens, completion_tokens, total_tokens, total_cost, status, created_at) VALUES (gen_random_uuid(), 'test', gen_random_uuid(), 'test-req', 'test-model', 'test', 'test', 100, 50, 150, 0.001, 'success', NOW());"
```

---

## 📊 监控与日志

### 实时日志

```bash
# 跟踪所有日志
tail -f gateway.log

# 仅计费相关
tail -f gateway.log | grep -E "billing|transaction"

# 错误日志
tail -f gateway.log | grep ERROR
```

### 数据库查询

```bash
# 连接数据库
psql $DATABASE_URL

# 查看最近 10 条计费记录
SELECT
  created_at,
  tenant_id,
  logical_model,
  total_tokens,
  total_cost,
  status
FROM billing_transactions
ORDER BY created_at DESC
LIMIT 10;

# 查看今日成本
SELECT
  tenant_id,
  COUNT(*) as requests,
  SUM(total_tokens) as tokens,
  SUM(total_cost) as cost
FROM billing_transactions
WHERE created_at >= CURRENT_DATE
GROUP BY tenant_id;

# 查看失败请求
SELECT * FROM billing_transactions
WHERE status != 'success'
ORDER BY created_at DESC
LIMIT 10;
```

### Prometheus 指标

```bash
# 访问 metrics 端点
curl http://localhost:8080/metrics

# 查看特定指标
curl http://localhost:8080/metrics | grep xjp_
```

---

## 🔄 更新部署

当有新代码时：

```bash
# 1. 拉取代码
git pull origin master

# 2. 检查是否有新迁移
sqlx migrate info

# 3. 运行新迁移（如有）
sqlx migrate run

# 4. 重新生成 SQLx 数据（如果 SQL 有变化）
cargo sqlx prepare

# 5. 重新构建
cargo build --release

# 6. 重启服务
pkill xjp-gateway
nohup ./target/release/xjp-gateway > gateway.log 2>&1 &
```

---

## 🎯 性能基准

在 C3 VM 上运行性能测试：

```bash
# 安装 wrk（如果未安装）
# Ubuntu: sudo apt install wrk
# CentOS: sudo yum install wrk

# 测试健康检查端点
wrk -t4 -c100 -d30s http://localhost:8080/healthz

# 测试计费查询（需要先创建测试数据）
wrk -t4 -c50 -d30s "http://localhost:8080/internal/billing/transactions?tenant_id=test&limit=10"
```

**预期指标**:
- 健康检查: > 10k req/s
- 计费查询: > 1k req/s
- 实际 LLM 请求: ~ 100 req/s（受限于上游 API）

---

## 📦 数据备份

定期备份计费数据：

```bash
# 创建备份脚本
cat > backup_billing.sh <<'EOF'
#!/bin/bash
DATE=$(date +%Y%m%d_%H%M%S)
BACKUP_DIR="/path/to/backups"
mkdir -p $BACKUP_DIR

# 备份整个数据库
pg_dump $DATABASE_URL > $BACKUP_DIR/xjp_gateway_$DATE.sql

# 或仅备份计费表
pg_dump $DATABASE_URL -t billing_transactions -t tenant_billing_summary > $BACKUP_DIR/billing_$DATE.sql

echo "Backup completed: $BACKUP_DIR/billing_$DATE.sql"
EOF

chmod +x backup_billing.sh

# 设置 cron（每天凌晨 2 点）
# crontab -e
# 0 2 * * * /path/to/backup_billing.sh
```

---

## 🔐 生产环境检查清单

- [ ] PostgreSQL 已优化（连接池、性能参数）
- [ ] 数据库已备份
- [ ] 环境变量已设置（不在代码中硬编码）
- [ ] OPENROUTER_API_KEY 已验证
- [ ] 日志轮转已配置（logrotate）
- [ ] 监控告警已设置（Prometheus + Grafana）
- [ ] 防火墙规则已配置（仅允许必要端口）
- [ ] SSL/TLS 证书已配置（如对外暴露）
- [ ] Rate limiting 已启用
- [ ] 计费数据归档策略已制定

---

## 📞 紧急联系

如遇问题，请检查：
1. `gateway.log` - 应用日志
2. PostgreSQL 日志 - `/var/log/postgresql/`
3. 系统日志 - `journalctl -u xjp-gateway`

---

**部署完成后，请在此打勾**：

- [ ] 代码已拉取
- [ ] 数据库已配置
- [ ] 迁移已运行
- [ ] 项目已构建
- [ ] 服务已启动
- [ ] 健康检查通过
- [ ] 计费测试通过

祝部署顺利！🚀
