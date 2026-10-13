FROM python:3.12-slim AS build
WORKDIR /site
COPY . .
RUN pip install --no-cache-dir markdown==3.7 && python build.py

FROM caddy:2-alpine
COPY Caddyfile /etc/caddy/Caddyfile
COPY --from=build /site/dist /srv
