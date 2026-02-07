/* SPDK headers for bindgen */

/* Environment & initialization */
#include <spdk/env.h>
#include <spdk/init.h>

/* Threading */
#include <spdk/thread.h>

/* Block device layer */
#include <spdk/bdev.h>
#include <spdk/bdev_module.h>

/* Blobstore */
#include <spdk/blob.h>
#include <spdk/blob_bdev.h>

/* NVMe driver */
#include <spdk/nvme.h>
#include <spdk/nvme_spec.h>

/* Utilities */
#include <spdk/log.h>
#include <spdk/string.h>
#include <spdk/json.h>

/* Event framework (optional, for app framework) */
#include <spdk/event.h>
