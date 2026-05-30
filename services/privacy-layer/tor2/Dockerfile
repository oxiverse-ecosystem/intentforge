FROM alpine:latest
RUN apk add --no-cache tor lyrebird su-exec
RUN mkdir -p /var/lib/tor && chown -R tor:tor /var/lib/tor
COPY torrc /etc/tor/torrc
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh
ENTRYPOINT ["/bin/sh", "/entrypoint.sh"]
