-- Dev seed: wipe and repopulate with demo data
-- Clerk user: user_3Ax62HMHNfbC2gyvCzBOMfB8tdb
-- Clerk org:  org_3Ax63TjpWPLZlHDNlqx9EndGIRr

-- -------------------------------------------------------
-- Wipe (preserve migration history)
-- -------------------------------------------------------
TRUNCATE TABLE
    notification_events,
    ticket_events,
    ticket_comments,
    notifications,
    tickets,
    occurrences,
    log_archives,
    logs,
    bugs,
    account_provider_configs,
    agents,
    accounts,
    memberships,
    users,
    organizations
CASCADE;

-- -------------------------------------------------------
-- Organization
-- -------------------------------------------------------
INSERT INTO organizations (id, name, clerk_org_id, created_at, updated_at) VALUES
    ('00000000-0000-0000-0000-000000000001', 'Daphne Dev Org', 'org_3Ax63TjpWPLZlHDNlqx9EndGIRr', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

-- -------------------------------------------------------
-- User
-- -------------------------------------------------------
INSERT INTO users (id, name, email, clerk_user_id, created_at, updated_at) VALUES
    ('00000000-0000-0000-0000-000000000002', 'Dev User', 'dev@bugfix.es', 'user_3Ax62HMHNfbC2gyvCzBOMfB8tdb', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

-- -------------------------------------------------------
-- Membership
-- -------------------------------------------------------
INSERT INTO memberships (id, organization_id, user_id, role, created_at, updated_at) VALUES
    ('00000000-0000-0000-0000-000000000003', '00000000-0000-0000-0000-000000000001', '00000000-0000-0000-0000-000000000002', 'admin', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

-- -------------------------------------------------------
-- Accounts (all under the same org)
-- -------------------------------------------------------
INSERT INTO accounts (id, organization_id, name, create_tickets, ticket_provider, notification_provider, notify_min_level, rapid_occurrence_window_minutes, rapid_occurrence_threshold, ai_enabled, use_managed_ai) VALUES
    ('127180e9-d498-4bf0-aebc-ce59a90f4842', '00000000-0000-0000-0000-000000000001', 'Chewedfeed',   1, 'none', 'none', 'error', 60, 10, 1, 1),
    ('9cdf702e-9a35-4783-936a-b085b70b8067', '00000000-0000-0000-0000-000000000001', 'Northstream',  1, 'none', 'none', 'error', 60, 10, 1, 1),
    ('711458eb-def5-4fa6-bb92-c74f88e8a9d7', '00000000-0000-0000-0000-000000000001', 'Northwind',    1, 'none', 'none', 'error', 60, 10, 1, 1);

-- -------------------------------------------------------
-- Agents (one per account)
-- -------------------------------------------------------
INSERT INTO agents (id, account_id, name, api_key, api_secret) VALUES
    ('cfb5b26c-3e64-4093-bb8f-fa9fc4999305', '127180e9-d498-4bf0-aebc-ce59a90f4842', 'chewedfeed-agent',  'key-chewedfeed-0001',  'secret-chewedfeed-0001'),
    ('5a6a2eef-ae83-4968-855d-c760b990da94', '9cdf702e-9a35-4783-936a-b085b70b8067', 'northstream-agent', 'key-northstream-0001', 'secret-northstream-0001'),
    ('2d1b63d3-a310-46f0-b294-8b581317ca04', '711458eb-def5-4fa6-bb92-c74f88e8a9d7', 'northwind-agent',   'key-northwind-0001',   'secret-northwind-0001');

-- -------------------------------------------------------
-- Bugs
-- -------------------------------------------------------
INSERT INTO bugs (id, account_id, agent_id, language, severity, stacktrace_hash, normalized_stacktrace, latest_stacktrace, first_seen_at, last_seen_at, occurrence_count) VALUES
(
    '0ecb8ffe-9be6-4f2a-8329-3febc7b824ed',
    '127180e9-d498-4bf0-aebc-ce59a90f4842',
    'cfb5b26c-3e64-4093-bb8f-fa9fc4999305',
    'python', 'fatal',
    'c1d4e7f0a3b6c9d2e5f8a1b4c7d0e3f6a9b2c5d8e1f4a7b0c3d6e9f2a5b8c1d4',
    'RuntimeError: CUDA out of memory
    ImagePipeline.generate:89
    BatchProcessor.run_batch:156
    WorkerPool.execute:42',
    'Traceback (most recent call last):
  File "/app/workers/pool.py", line 42, in execute
    result = self.processor.run_batch(batch)
  File "/app/workers/batch_processor.py", line 156, in run_batch
    output = self.pipeline.generate(inputs)
  File "/app/ml/image_pipeline.py", line 89, in generate
    tensor = self.model.forward(preprocessed)
  File "/app/venv/lib/python3.11/site-packages/torch/nn/modules/module.py", line 1518, in _wrapped_call_impl
    return self._call_impl(*args, **kwargs)
RuntimeError: CUDA error: out of memory
Tried to allocate 2.40 GiB (GPU 0; 23.65 GiB total capacity; 21.14 GiB already allocated)',
    '2026-03-15T03:22:00+00:00', '2026-03-16T01:15:44+00:00', 7
),
(
    '4c66367d-ab23-45a6-b94d-1a6e58cae70c',
    '127180e9-d498-4bf0-aebc-ce59a90f4842',
    'cfb5b26c-3e64-4093-bb8f-fa9fc4999305',
    'python', 'error',
    'f2a5b8c1d4e7f0a3b6c9d2e5f8a1b4c7d0e3f6a9b2c5d8e1f4a7b0c3d6e9f2a5',
    'IntegrityError: duplicate key value violates unique constraint
    AccountRepository.create:56
    OnboardingService.provision:112
    handle_signup:28',
    'Traceback (most recent call last):
  File "/app/api/routes/signup.py", line 28, in handle_signup
    account = await onboarding.provision(payload)
  File "/app/services/onboarding.py", line 112, in provision
    return await self.repo.create(account_data)
  File "/app/repositories/account.py", line 56, in create
    result = await self.db.execute(query, values)
  File "/app/venv/lib/python3.11/site-packages/asyncpg/connection.py", line 318, in execute
    return await self._protocol.query(query, timeout)
asyncpg.exceptions.UniqueViolationError: duplicate key value violates unique constraint "accounts_email_key"
DETAIL: Key (email)=(onboard@chewedfeed.io) already exists.',
    '2026-03-11T16:45:22+00:00', '2026-03-15T21:08:14+00:00', 19
),
(
    'c721e0b5-deb3-48a7-bb60-a19aa26e4936',
    '127180e9-d498-4bf0-aebc-ce59a90f4842',
    'cfb5b26c-3e64-4093-bb8f-fa9fc4999305',
    'ruby', 'error',
    'c3d6e9f2a5b8c1d4e7f0a3b6c9d2e5f8a1b4c7d0e3f6a9b2c5d8e1f4a7b0c3d6',
    'Timeout::Error: execution expired
    HttpAdapter#post:44
    WebhookService#deliver:89
    EventProcessor#handle:23',
    'Timeout::Error (execution expired):
  lib/http_adapter.rb:44:in `post''
  app/services/webhook_service.rb:89:in `deliver''
  app/processors/event_processor.rb:23:in `handle''
  app/jobs/event_worker.rb:12:in `perform''
  activejob (7.1.3) lib/active_job/execution.rb:53:in `perform_now''
  activejob (7.1.3) lib/active_job/execution.rb:23:in `block in execute''
  sidekiq (7.2.1) lib/sidekiq/job_retry.rb:113:in `local''',
    '2026-03-08T04:15:00+00:00', '2026-03-16T07:58:03+00:00', 42
),
(
    '170ba9c1-e94a-4a25-8288-67d556c61a89',
    '9cdf702e-9a35-4783-936a-b085b70b8067',
    '5a6a2eef-ae83-4968-855d-c760b990da94',
    'go', 'error',
    'e5f8a1b4c7d0e3f6a9b2c5d8e1f4a7b0c3d6e9f2a5b8c1d4e7f0a3b6c9d2e5f8',
    'panic: send on closed channel
    (*EventBus).Publish:67
    (*NotificationWorker).dispatch:134
    (*WorkerPool).Run:89',
    'goroutine 112 [running]:
main.(*EventBus).Publish(0xc0000b6000, {0xc000218060, 0x1c})
    /app/internal/events/bus.go:67 +0x109
main.(*NotificationWorker).dispatch(0xc000194000, 0xc0001dc000)
    /app/internal/workers/notification.go:134 +0x245
main.(*WorkerPool).Run.func1()
    /app/internal/workers/pool.go:89 +0x8a
created by main.(*WorkerPool).Run in goroutine 1
    /app/internal/workers/pool.go:78 +0x105',
    '2026-03-15T19:08:11+00:00', '2026-03-16T02:33:55+00:00', 3
),
(
    '67f0f60c-b9c6-48ef-bd4c-52e3461e06b5',
    '9cdf702e-9a35-4783-936a-b085b70b8067',
    '5a6a2eef-ae83-4968-855d-c760b990da94',
    'rust', 'fatal',
    'a1b4c7d0e3f6a9b2c5d8e1f4a7b0c3d6e9f2a5b8c1d4e7f0a3b6c9d2e5f8a1b4',
    'thread panicked: index out of bounds: len is 0 but index is 0
    ConnectionPool::checkout:187
    HttpClient::send:94
    WebhookDispatcher::fire:42',
    'thread ''tokio-runtime-worker'' panicked at ''index out of bounds: the len is 0 but the index is 0'', src/pool/connections.rs:187:22
stack backtrace:
   0: std::panicking::begin_panic_handler
   1: core::panicking::panic_fmt
   2: core::panicking::panic_bounds_check
   3: daphne::pool::connections::ConnectionPool::checkout
             at ./src/pool/connections.rs:187:22
   4: daphne::http::client::HttpClient::send
             at ./src/http/client.rs:94:18
   5: daphne::webhooks::dispatcher::WebhookDispatcher::fire
             at ./src/webhooks/dispatcher.rs:42:9
   6: tokio::runtime::task::core::Core<T,S>::poll
             at /rustc/hash/library/tokio/src/runtime/task/core.rs:184:17',
    '2026-03-16T06:11:00+00:00', '2026-03-16T06:44:22+00:00', 2
),
(
    '8067b41c-77d3-4cd7-b505-3854d8720578',
    '9cdf702e-9a35-4783-936a-b085b70b8067',
    '5a6a2eef-ae83-4968-855d-c760b990da94',
    'go', 'warn',
    'd6e9f2a5b8c1d4e7f0a3b6c9d2e5f8a1b4c7d0e3f6a9b2c5d8e1f4a7b0c3d6e9',
    'context deadline exceeded
    (*GRPCClient).FetchMetrics:78
    (*MetricsCollector).Collect:45
    (*Scheduler).tick:112',
    'rpc error: code = DeadlineExceeded desc = context deadline exceeded
goroutine 89 [running]:
main.(*GRPCClient).FetchMetrics(0xc0001b2000, {0x7f4a2c1d8860, 0xc000244000})
    /app/internal/metrics/grpc.go:78 +0x1f4
main.(*MetricsCollector).Collect(0xc000196000)
    /app/internal/metrics/collector.go:45 +0xba
main.(*Scheduler).tick(0xc0001a0000)
    /app/internal/scheduler/scheduler.go:112 +0x15c',
    '2026-03-05T12:00:00+00:00', '2026-03-16T10:01:44+00:00', 156
),
(
    'a8c13c72-1435-4164-be30-69dfe929512e',
    '9cdf702e-9a35-4783-936a-b085b70b8067',
    '5a6a2eef-ae83-4968-855d-c760b990da94',
    'rust', 'error',
    'da1491e9b29408b5e9ec83d1a95bdf421c28368ea78cec0a3111476396e57122',
    'panic: nil pointer dereference',
    'panic: nil pointer dereference',
    '2026-03-14T15:01:23.631919+00:00', '2026-03-14T15:01:23.631919+00:00', 1
),
(
    'f34fba99-401e-47ca-ba07-5340c692a52f',
    '9cdf702e-9a35-4783-936a-b085b70b8067',
    '5a6a2eef-ae83-4968-855d-c760b990da94',
    'go', 'error',
    '8a4f1c92d3e7b0a5f6c8d1e4a7b3f9c2d5e8a1b4c7d0e3f6a9b2c5d8e1f4a7b0',
    'panic: nil pointer dereference
    (*CheckoutHandler).ProcessOrder:142
    (*CheckoutHandler).ServeHTTP:87
    (*Router).dispatch:203',
    'goroutine 47 [running]:
main.(*CheckoutHandler).ProcessOrder(0xc0001a4000, {0xc000258000, 0x3, 0x3})
    /app/internal/handlers/checkout.go:142 +0x2a5
main.(*CheckoutHandler).ServeHTTP(0xc0001a4000, {0x7f8a2c1d88e0, 0xc0002a6000}, 0xc000284300)
    /app/internal/handlers/checkout.go:87 +0x1bc
main.(*Router).dispatch(0xc000138000, {0x7f8a2c1d88e0, 0xc0002a6000}, 0xc000284300)
    /app/internal/router.go:203 +0x4f2
net/http.serverHandler.ServeHTTP({0xc000138000}, {0x7f8a2c1d88e0, 0xc0002a6000}, 0xc000284300)
    /usr/local/go/src/net/http/server.go:2936 +0x316',
    '2026-03-12T09:14:22+00:00', '2026-03-15T16:42:11+00:00', 14
),
(
    '3ebb7529-aa04-4a4d-8484-5bfa3215ccac',
    '711458eb-def5-4fa6-bb92-c74f88e8a9d7',
    '2d1b63d3-a310-46f0-b294-8b581317ca04',
    'typescript', 'warn',
    'd8e1f4a7b0c3d6e9f2a5b8c1d4e7f0a3b6c9d2e5f8a1b4c7d0e3f6a9b2c5d8e1',
    'Error: ECONNREFUSED 127.0.0.1:6379
    RedisClient.connect:34
    CacheService.get:18
    SessionStore.load:72',
    'Error: connect ECONNREFUSED 127.0.0.1:6379
    at TCPConnectWrap.afterConnect [as oncomplete] (node:net:1595:16)
    at RedisClient.connect (/app/src/lib/redis.ts:34:11)
    at CacheService.get (/app/src/services/cache.ts:18:24)
    at SessionStore.load (/app/src/middleware/session-store.ts:72:20)
    at Layer.handle (/app/node_modules/express/lib/router/layer.js:95:5)',
    '2026-03-10T22:14:00+00:00', '2026-03-16T09:44:31+00:00', 88
),
(
    '44ead7c5-7433-4699-83f8-83212186ef24',
    '711458eb-def5-4fa6-bb92-c74f88e8a9d7',
    '2d1b63d3-a310-46f0-b294-8b581317ca04',
    'java', 'error',
    'b0c3d6e9f2a5b8c1d4e7f0a3b6c9d2e5f8a1b4c7d0e3f6a9b2c5d8e1f4a7b0c3',
    'java.lang.OutOfMemoryError: Java heap space
    ReportGenerator.buildPdf:312
    ReportController.export:78
    DispatcherServlet.doDispatch:1067',
    'java.lang.OutOfMemoryError: Java heap space
    at java.base/java.util.Arrays.copyOf(Arrays.java:3512)
    at java.base/java.io.ByteArrayOutputStream.grow(ByteArrayOutputStream.java:120)
    at com.northwind.reports.ReportGenerator.buildPdf(ReportGenerator.java:312)
    at com.northwind.controllers.ReportController.export(ReportController.java:78)
    at java.base/jdk.internal.reflect.NativeMethodAccessorImpl.invoke0(Native Method)
    at org.springframework.web.servlet.FrameworkServlet.service(FrameworkServlet.java:897)
    at org.springframework.web.servlet.DispatcherServlet.doDispatch(DispatcherServlet.java:1067)
    at javax.servlet.http.HttpServlet.service(HttpServlet.java:750)',
    '2026-03-14T10:30:00+00:00', '2026-03-15T17:22:18+00:00', 6
),
(
    'fd407d6b-3faa-4422-b63f-ca7d5c9d9e41',
    '711458eb-def5-4fa6-bb92-c74f88e8a9d7',
    '2d1b63d3-a310-46f0-b294-8b581317ca04',
    'typescript', 'error',
    'b7e3f1a9c2d5e8a1b4c7d0e3f6a9b2c5d8e1f4a7b0c3d6e9f2a5b8c1d4e7f0a3',
    'TypeError: Cannot read properties of undefined (reading ''email'')
    UserService.getProfile:48
    AuthController.me:22
    asyncHandler:15',
    'TypeError: Cannot read properties of undefined (reading ''email'')
    at UserService.getProfile (/app/src/services/user.ts:48:32)
    at AuthController.me (/app/src/controllers/auth.ts:22:28)
    at asyncHandler (/app/src/middleware/async.ts:15:12)
    at Layer.handle (/app/node_modules/express/lib/router/layer.js:95:5)
    at next (/app/node_modules/express/lib/router/route.js:149:14)
    at AuthMiddleware.verify (/app/src/middleware/auth.ts:31:5)
    at processTickByOrder (/app/node_modules/express/lib/router/index.js:284:7)',
    '2026-03-13T14:08:33+00:00', '2026-03-16T08:21:47+00:00', 31
);
