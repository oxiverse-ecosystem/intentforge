# Exposing IntentForge with a Cloudflare Tunnel

`cloudflared` runs as an **opt-in Docker Compose service** (`profile: tunnel`). It shares the gluetun network namespace, so tunnel traffic egresses through the VPN and the origin URL is `http://localhost:4000` — not `http://gateway:4000` (service-name DNS does not resolve inside a shared netns).

Official docs used: [Set up Cloudflare Tunnel](https://developers.cloudflare.com/tunnel/get-started/) and [Tunnel run parameters](https://developers.cloudflare.com/tunnel/reference/run-parameters/) (`TUNNEL_TOKEN` is the supported environment variable for `tunnel run`).

## 1. Create the tunnel and get a token

1. Log in to the [Cloudflare dashboard](https://dash.cloudflare.com/) → **Networking** → **Tunnels** → **Create a tunnel**.
2. Name it (for example `intentforge-dev`).
3. Under **Setup Environment**, choose **Docker**. The install command shown contains the tunnel token — copy the token value.
4. Leave the tunnel page open until the connector reports **Healthy**.

A domain managed by Cloudflare is required for a public hostname.

## 2. Put the token in `.env`

In the repo root `.env` (git-ignored), add or replace:

```
TUNNEL_TOKEN=<paste the token from step 1>
```

Never commit this value — it authorizes a connector to your tunnel. To rotate it later, regenerate the token in the dashboard, update `.env`, and recreate the container.

## 3. Add the public hostname route

In the tunnel's **Routes** tab → **Add route** → **Published application**:

- **Hostname:** your chosen subdomain, e.g. `search.example.com`
- **Service URL:** `http://localhost:4000` ← important, see the netns note above
- (optional) **Origin Server Name:** leave blank for plain HTTP origin

Cloudflare creates the proxied CNAME automatically for a hostname on a zone in your account. If you use a different zone, add a CNAME to `<TUNNEL_ID>.cfargotunnel.com` yourself.

## 4. Start the connector

Run from the `services/` directory (relative `../.env` paths depend on it):

```bash
# Dev
cd services
docker compose -f docker-compose.dev.yml --profile tunnel up -d cloudflared
docker logs -f if-dev-cloudflared

# Prod equivalent
docker compose -f docker-compose.prod.yml --profile tunnel up -d cloudflared
docker logs -f if-cloudflared
```

A healthy connector logs `Registered tunnel connection`; the dashboard also flips the tunnel to **Healthy**.

Verify from outside your network:

```bash
curl https://search.example.com/health    # expect OK
curl 'https://search.example.com/search/fast?q=test&limit=3'
```

## 5. Protect it before inviting testers

The search endpoints are public by design. `/goals`, `/shopping` and `/commerce/extract` require the `x-api-key` header and must stay behind Cloudflare Access or an API gateway if you expose them.

For a small set of real testers, add a Cloudflare **Access application** for the hostname with an email/PIN policy (Zero Trust → Access → Applications). That keeps the gateway auth model unchanged while limiting who can reach it. Cloudflare also applies WAF and DDoS protection in front of the origin.

## Troubleshooting

- **Tunnel stays Down / `failed to connect`:** the compose service forces `--protocol http2` because gluetun's VPN rotation briefly blocks QUIC/UDP (`sendmsg: operation not permitted`). Do not switch back to `auto` on this stack; TCP/443 survives the rotation. Recreate with `docker compose -f docker-compose.dev.yml --profile tunnel up -d --no-deps --force-recreate cloudflared` after changing the command.
- **502 at the hostname:** the route Service URL must be `http://localhost:4000`; `http://gateway:4000` will not resolve in the shared netns. Confirm the gateway itself responds with `curl -s http://localhost:4000/health` from the `gluetun` container.
- **`TUNNEL_TOKEN` not applied:** compose reads `../.env` via `env_file` because it is invoked from `services/`; check with `docker exec if-dev-cloudflared printenv TUNNEL_TOKEN` (prints the value — avoid pasting it anywhere).
- **Recreating `gluetun` orphans netns sharers:** after changing gluetun, force-recreate `cloudflared` and other `network_mode: service:gluetun` containers, then re-check `docker exec <c> readlink /proc/self/ns/net`.
- **Stop exposing:** `docker compose -f docker-compose.dev.yml --profile tunnel stop cloudflared` (or remove the `tunnel` profile start). No other service is affected; the stack does not start cloudflared by default.
