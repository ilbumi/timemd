# The binary is not built here. It is lifted out of the release tarball, so the
# image and the tarball are the same bytes and there is no second compile to
# drift. `.github/workflows/build-image.yml` stages `<arch>/timemd` and hands
# this file that directory as the whole build context — which is also why the
# COPY path has no `dist/` prefix and why there is no .dockerignore: `target/`
# and `node_modules/` are never in scope to exclude.
#
# Nothing here RUNs, so buildx produces both architectures on one runner with no
# emulation.
FROM gcr.io/distroless/cc-debian13:nonroot

ARG TARGETARCH
COPY ${TARGETARCH}/timemd /usr/local/bin/timemd

# WORKDIR creates /data owned by the image's user (65532), which is what lets a
# non-root container write state/push.md at mode 0600. A `RUN mkdir` would need
# a shell this base does not have. A bind mount brings its own ownership, so the
# README tells you to chown it.
#
# No VOLUME: the documented way to run this always passes `-v`, and declaring one
# would only change the undocumented case, silently stashing data in an anonymous
# volume rather than losing it with the container as a reader would expect.
WORKDIR /data

ENV TIMEMD_DATA=/data \
    TIMEMD_ADDR=0.0.0.0:8080
EXPOSE 8080

# No HEALTHCHECK: there is no shell to run one. /api/health is there for
# whatever orchestrator wants to probe it.
#
# The entrypoint is the binary rather than `timemd serve`, so every subcommand
# the tarball offers works here too: `docker run … status`, `… --version`.
ENTRYPOINT ["/usr/local/bin/timemd"]
CMD ["serve"]
