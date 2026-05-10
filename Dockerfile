# KitsuneEngine Demo Pitch Deck
# nginx listens on $PORT (injected by Render at runtime, defaults to 8080/10000)
#
# Run locally:
#   docker build -t kitsune-demo .
#   PORT=8080 docker run -p 8080:8080 -e PORT=8080 kitsune-demo

FROM nginx:alpine

# Remove default placeholder
RUN rm -rf /usr/share/nginx/html/*

# Copy presentation files
COPY KitsuneEngine_Demo_Video.html /usr/share/nginx/html/
COPY index.html                    /usr/share/nginx/html/

# Use nginx's template mechanism — envsubst replaces $PORT before nginx starts
# Templates in /etc/nginx/templates/ are processed automatically on container start
RUN mkdir -p /etc/nginx/templates && \
    printf 'server {\n\
    listen ${PORT};\n\
    root /usr/share/nginx/html;\n\
    index index.html;\n\
    gzip on;\n\
    gzip_types text/html text/css application/javascript;\n\
    add_header Cache-Control "no-cache, must-revalidate";\n\
    location / {\n\
        try_files $uri $uri/ /index.html;\n\
    }\n\
}\n' > /etc/nginx/templates/default.conf.template

# Render sets $PORT at runtime — expose the same default for local dev
EXPOSE 8080

CMD ["nginx", "-g", "daemon off;"]
