# Serves a folder of static files (the documentation site) with nginx, as a non-root user on 8080.
# Nothing is built here: `nx run <app>:build` produces DIST_DIR first (container targets depend on it).
FROM docker.io/nginxinc/nginx-unprivileged:stable-alpine

ARG DIST_DIR
# The official image turns files in /etc/nginx/templates into config at start-up, filling in the
# environment variables below.
ENV DEMO_UPSTREAM=demo:3000 \
    DNS_RESOLVER=127.0.0.11
COPY packages/docker/static.nginx.conf.template /etc/nginx/templates/default.conf.template
COPY --chown=101:101 ${DIST_DIR} /usr/share/nginx/html

EXPOSE 8080
