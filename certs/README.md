# Extra CA certificates for Docker builds

Drop corporate / proxy root CA certificates here as `*.crt` (PEM format).
The Dockerfile's build stage adds them to the system trust store so `cargo`
can reach crates.io through an HTTPS-inspecting proxy (e.g. Netskope, Zscaler).

- SSL verification stays ON — we trust the proxy's CA instead of disabling checks.
- `*.crt` files are git-ignored: they're specific to your network, and a build
  outside the corporate network (CI, cloud) works fine with this folder empty.

To re-extract the Netskope certs if they rotate:

    docker run --rm debian:trixie-slim sh -c 'apt-get -qq update && apt-get -qq install -y openssl >/dev/null; \
      echo | openssl s_client -connect index.crates.io:443 -showcerts 2>/dev/null'

Save the `CertServiceCA` (root) and `ca.<org>.goskope.com` (intermediate) PEM blocks here.
