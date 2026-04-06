FROM node:20-bookworm-slim AS deps
WORKDIR /app
COPY package.json tsconfig.json tsup.config.ts ./
COPY src ./src
COPY static ./static
COPY scripts ./scripts
RUN npm install
RUN npm run build

FROM node:20-bookworm-slim AS runtime
WORKDIR /app
ENV NODE_ENV=production
COPY --from=deps /app/package.json ./package.json
COPY --from=deps /app/node_modules ./node_modules
COPY --from=deps /app/dist ./dist
COPY static ./static
COPY examples ./examples
EXPOSE 7761
CMD ["node", "dist/cli.mjs", "serve", "--bind", "0.0.0.0:7761"]
