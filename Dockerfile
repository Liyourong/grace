FROM rust:1-trixie AS build
WORKDIR /src
COPY backend/Cargo.toml backend/Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs && cargo build --release && rm -rf src
COPY backend/ ./
RUN touch src/main.rs && cargo build --release

FROM node:24-slim AS web
WORKDIR /web
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ ./
RUN npm run build

FROM debian:trixie-slim
COPY --from=build /src/target/release/grace /usr/local/bin/grace
COPY --from=web /web/dist/ /app/frontend/
ENV STATIC_DIR=/app/frontend PORT=8080
EXPOSE 8080
USER nobody
CMD ["grace"]
