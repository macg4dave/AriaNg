# syntax=docker/dockerfile:1
FROM node:24-bookworm-slim AS build
WORKDIR /app
COPY package.json package-lock.json ./
# Git is required by the locked angular-input-dropdown dependency.
RUN apt-get update \
    && apt-get install -y --no-install-recommends git ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && npm ci --no-audit --no-fund
COPY gulpfile.js .eslintrc.json LICENSE ./
COPY src/ ./src/
RUN npm run build

FROM debian:bookworm-slim AS aria2
RUN apt-get update \
    && apt-get install -y --no-install-recommends aria2 ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
COPY docker/aria2.conf /etc/aria2/aria2.conf
COPY docker/aria2-start.sh docker/aria2-healthcheck.sh /usr/local/bin/
USER www-data
HEALTHCHECK --interval=5s --timeout=3s --start-period=5s --retries=5 \
    CMD ["sh", "/usr/local/bin/aria2-healthcheck.sh"]
ENTRYPOINT ["sh", "/usr/local/bin/aria2-start.sh"]

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends nginx ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* /var/www/html/* \
    && dd if=/dev/zero of=/opt/test-fixture.bin bs=1024 count=64 status=none
COPY docker/nginx.conf /etc/nginx/nginx.conf
COPY --from=build /app/dist/ /var/www/html/
USER www-data
EXPOSE 8080
STOPSIGNAL SIGQUIT
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD ["curl", "--fail", "--silent", "--show-error", "--max-time", "2", "http://127.0.0.1:8080/index.html", "--output", "/dev/null"]
CMD ["nginx", "-g", "daemon off;"]
