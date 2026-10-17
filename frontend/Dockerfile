FROM nginxinc/nginx-unprivileged:stable-alpine

COPY frontend/nginx.conf /etc/nginx/conf.d/default.conf
COPY frontend/index.html frontend/styles.css frontend/app.js frontend/generator-worker.js /usr/share/nginx/html/

EXPOSE 8080
