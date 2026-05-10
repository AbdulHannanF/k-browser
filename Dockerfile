# ── KitsuneEngine Demo Pitch Deck ──────────────────────────────────────────
# Serves the static HTML presentation via nginx on port 80.
# Deploy to Render as a "Web Service" (Docker), or run locally:
#   docker build -t kitsune-demo .
#   docker run -p 8080:80 kitsune-demo
# ---------------------------------------------------------------------------

FROM nginx:alpine

# Remove the default nginx placeholder page
RUN rm -rf /usr/share/nginx/html/*

# Copy the presentation and supporting files
COPY KitsuneEngine_Demo_Video.html /usr/share/nginx/html/
COPY index.html                    /usr/share/nginx/html/

# Minimal nginx config: serve on port 80, gzip on, cache headers
RUN printf 'server {\n\
    listen 80;\n\
    root /usr/share/nginx/html;\n\
    index index.html;\n\
    gzip on;\n\
    gzip_types text/html text/css application/javascript;\n\
    add_header Cache-Control "no-cache, must-revalidate";\n\
    location / {\n\
        try_files $uri $uri/ /index.html;\n\
    }\n\
}\n' > /etc/nginx/conf.d/default.conf

EXPOSE 80

CMD ["nginx", "-g", "daemon off;"]
