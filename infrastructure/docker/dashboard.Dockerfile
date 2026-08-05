FROM node:24-alpine AS builder

WORKDIR /workspace
RUN corepack enable

ARG VITE_AUTH0_DOMAIN
ARG VITE_AUTH0_CLIENT_ID
ARG VITE_AUTH0_AUDIENCE
ENV VITE_AUTH0_DOMAIN=$VITE_AUTH0_DOMAIN
ENV VITE_AUTH0_CLIENT_ID=$VITE_AUTH0_CLIENT_ID
ENV VITE_AUTH0_AUDIENCE=$VITE_AUTH0_AUDIENCE

COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY apps/dashboard/package.json apps/dashboard/package.json
COPY packages/ui/package.json packages/ui/package.json
COPY packages/api-client/package.json packages/api-client/package.json
RUN pnpm install --frozen-lockfile

COPY apps/dashboard ./apps/dashboard
COPY packages/ui ./packages/ui
COPY packages/api-client ./packages/api-client
RUN pnpm --filter @techatlas/dashboard build

FROM nginxinc/nginx-unprivileged:1.28-alpine

COPY infrastructure/nginx/dashboard.conf /etc/nginx/conf.d/default.conf
COPY --from=builder /workspace/apps/dashboard/dist /usr/share/nginx/html
