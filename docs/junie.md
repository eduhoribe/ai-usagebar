# Junie (JetBrains)

Create a Junie API key at [Junie Tokens](https://junie.jetbrains.com/cli),
enable Junie in Settings, and paste the key there. Alternatively:

```toml
[junie]
enabled = true
api_key_env = "JUNIE_API_KEY"
```

Set `JUNIE_API_KEY` in the environment inherited by the status bar, then run
`ai-usagebar --vendor junie` or `ai-usagebar usage --json`.
The widget placeholder is `{junie_balance}`; `{plan}` reports the plan.

Named accounts use the standard API-key account configuration:

```toml
[junie]
enabled = true
show_default_account = false

[[junie.accounts]]
label = "work"
api_key_env = "JUNIE_WORK_API_KEY"
```

This provider reads the account endpoint used by Junie CLI's `/stats`:
`GET https://ingrazzio-cloud-prod.labs.jb.gg/auth/test`, authenticated with
`Authorization: Bearer <Junie API key>`. It never starts a model request,
renews credentials, or writes to Junie's files. Model-vendor BYOK keys and
JetBrains organization analytics tokens are not Junie API keys. CLI OAuth,
IDE-only logins, and enterprise gateways are not supported.

The request and conversion were checked against the installed official
Junie CLI 3893.6 (`ProductionEnvironment`, `checkIngrazzioInteractive`,
`IngrazzioAuthInfo`, and `FormatCostKt.formatCreditsBalance`). This is an
undocumented endpoint and may change. Its `balanceLeft` is already in AI
credits when `balanceUnit` is `USD`; other units use the CLI's division by
100,000. A `GZL` license does not expose an AI credit balance. Missing or
invalid balances produce an error rather than a zero reading.

No quota denominator or reset date is supplied, so the provider shows the
credit balance without a percentage or countdown. Only the projected
balance, plan, and a token fingerprint are cached. Failed refreshes can show
the last good reading through the shared stale-cache policy; changing keys
cannot reuse another key's reading. Redirects are refused, and response
bodies are omitted from errors.
