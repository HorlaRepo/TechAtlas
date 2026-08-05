FROM postgres:16.11-alpine

RUN apk add --no-cache bash coreutils rclone

COPY infrastructure/backup/backup.sh /backup/backup.sh
COPY infrastructure/backup/restore.sh /backup/restore.sh
RUN chmod 0555 /backup/backup.sh /backup/restore.sh

ENTRYPOINT ["/bin/sh"]
