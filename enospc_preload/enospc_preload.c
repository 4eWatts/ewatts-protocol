/*
 * enospc_preload.c — LD_PRELOAD library that intercepts write()/pwrite()/fwrite()
 * and returns ENOSPC after a configurable total byte limit.
 * Usage: ENOSPC_LIMIT=500000 LD_PRELOAD=./enospc_preload.so ./ewatts-protocol start ...
 *
 * Compile: gcc -shared -fPIC -o enospc_preload.so enospc_preload.c -ldl
 */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

/* Track total bytes written across all write-like calls */
static size_t total_written = 0;
static size_t write_limit = 0;
static int initialized = 0;

static void init_limit(void) {
    if (initialized) return;
    initialized = 1;
    const char *env = getenv("ENOSPC_LIMIT");
    if (env) {
        write_limit = (size_t)atoll(env);
        fprintf(stderr, "[enospc_preload] write limit set to %zu bytes\n", write_limit);
    } else {
        write_limit = 0; /* unlimited */
    }
}

/* Check if we've hit the limit and need to fail */
static int check_limit(size_t len) {
    if (write_limit == 0) return 0; /* no limit */
    if (total_written >= write_limit) {
        errno = ENOSPC;
        return -1;
    }
    if (total_written + len > write_limit) {
        /* Partial: write up to the limit would succeed, rest would fail.
         * For simplicity, fail the entire write once we'd exceed. */
        errno = ENOSPC;
        return -1;
    }
    return 0; /* OK to write */
}

ssize_t write(int fd, const void *buf, size_t count) {
    static ssize_t (*real_write)(int, const void *, size_t) = NULL;
    if (!real_write) {
        real_write = (ssize_t (*)(int, const void *, size_t))dlsym(RTLD_NEXT, "write");
    }
    init_limit();
    if (check_limit(count)) return -1;
    ssize_t ret = real_write(fd, buf, count);
    if (ret > 0) total_written += ret;
    return ret;
}

ssize_t pwrite(int fd, const void *buf, size_t count, off_t offset) {
    static ssize_t (*real_pwrite)(int, const void *, size_t, off_t) = NULL;
    if (!real_pwrite) {
        real_pwrite = (ssize_t (*)(int, const void *, size_t, off_t))dlsym(RTLD_NEXT, "pwrite");
    }
    init_limit();
    if (check_limit(count)) return -1;
    ssize_t ret = real_pwrite(fd, buf, count, offset);
    if (ret > 0) total_written += ret;
    return ret;
}

size_t fwrite(const void *ptr, size_t size, size_t nmemb, FILE *stream) {
    static size_t (*real_fwrite)(const void *, size_t, size_t, FILE *) = NULL;
    if (!real_fwrite) {
        real_fwrite = (size_t (*)(const void *, size_t, size_t, FILE *))dlsym(RTLD_NEXT, "fwrite");
    }
    init_limit();
    size_t total = size * nmemb;
    if (check_limit(total)) return 0;
    size_t ret = real_fwrite(ptr, size, nmemb, stream);
    if (ret > 0) total_written += ret * size;
    return ret;
}

int fflush(FILE *stream) {
    static int (*real_fflush)(FILE *) = NULL;
    if (!real_fflush) {
        real_fflush = (int (*)(FILE *))dlsym(RTLD_NEXT, "fflush");
    }
    init_limit();
    return real_fflush(stream);
}

int close(int fd) {
    static int (*real_close)(int) = NULL;
    if (!real_close) {
        real_close = (int (*)(int))dlsym(RTLD_NEXT, "close");
    }
    return real_close(fd);
}
