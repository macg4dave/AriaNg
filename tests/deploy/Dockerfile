FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends openssh-server nginx curl ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /run/sshd /root/.ssh /srv/ariang-release \
    && chmod 700 /root/.ssh \
    && printf 'server { listen 8080; root /srv/ariang-release/current; location / { try_files $uri =404; } }\n' > /etc/nginx/sites-enabled/default \
    && printf 'PermitRootLogin prohibit-password\nPasswordAuthentication no\nUsePAM no\nSubsystem sftp internal-sftp\n' > /etc/ssh/sshd_config
COPY start.sh /start.sh
CMD ["sh", "/start.sh"]
