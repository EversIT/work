FROM docker.io/node:alpine3.24 AS runner

RUN apk add --no-cache dumb-init

ENV NODE_ENV=production
ENV PORT=3000
ENV HOSTNAME=0.0.0.0
ENV NEXT_TELEMETRY_DISABLED=1

EXPOSE 3000

WORKDIR /usr/src/app

ARG APP_NAME
ENV APP_NAME=${APP_NAME}
# The build output folder under dist/apps.
ARG DIST_NAME=${APP_NAME}

COPY --chown=node:node dist/apps/${DIST_NAME}/standalone ./
COPY --chown=node:node dist/apps/${DIST_NAME}/static ./dist/apps/${DIST_NAME}/static
COPY --chown=node:node apps/${APP_NAME}/public ./apps/${APP_NAME}/public

USER node

ENTRYPOINT ["dumb-init", "--"]
CMD ["sh", "-c", "node apps/${APP_NAME}/server.js"]
