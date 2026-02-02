terraform {
  required_version = ">= 1.0"

  required_providers {
    libvirt = {
      source  = "dmacvicar/libvirt"
      version = "~> 0.9"
    }
    null = {
      source  = "hashicorp/null"
      version = "~> 3.0"
    }
  }
}

# Configure the Libvirt provider
provider "libvirt" {
  uri = var.libvirt_uri
}

# Load SSH key from file if not provided directly
locals {
  ssh_public_key = var.ssh_public_key != "" ? var.ssh_public_key : file(pathexpand(var.ssh_key_path))
}

# Base OS image volume (downloaded from URL)
resource "libvirt_volume" "base_image" {
  name = "${var.vm_name}-base.qcow2"
  pool = var.storage_pool

  create = {
    content = {
      url = var.base_image_url
    }
  }
}

# VM disk volume (overlay on base image)
resource "libvirt_volume" "vm_disk" {
  name     = "${var.vm_name}-disk.qcow2"
  pool     = var.storage_pool
  capacity = var.disk_size

  target = {
    format = {
      type = "qcow2"
    }
  }

  backing_store = {
    path = libvirt_volume.base_image.path
    format = {
      type = "qcow2"
    }
  }
}

# Cloud-init configuration
resource "libvirt_cloudinit_disk" "cloudinit" {
  name = "${var.vm_name}-cloudinit"

  meta_data = yamlencode({
    instance-id    = "${var.vm_name}-${formatdate("YYYYMMDDhhmmss", timestamp())}"
    local-hostname = var.vm_name
  })

  user_data = <<-EOF
    #cloud-config
    hostname: ${var.vm_name}
    users:
      - name: ${var.ssh_user}
        sudo: ALL=(ALL) NOPASSWD:ALL
        shell: /bin/bash
        ssh_authorized_keys:
          - ${local.ssh_public_key}
    package_update: false
  EOF

  network_config = yamlencode({
    version = 2
    ethernets = {
      id0 = {
        match = {
          name = "en*"
        }
        dhcp4 = true
      }
    }
  })
}

# Cloud-init volume
resource "libvirt_volume" "cloudinit" {
  name = "${var.vm_name}-cloudinit.iso"
  pool = var.storage_pool

  create = {
    content = {
      url = libvirt_cloudinit_disk.cloudinit.path
    }
  }

  lifecycle {
    replace_triggered_by = [libvirt_cloudinit_disk.cloudinit]
  }
}

# NVMe disk - created in /tmp for easy permissions (world-writable)
# QEMU can access /tmp without needing special directory permissions
locals {
  nvme_disk_path = var.nvme_enabled ? "/tmp/${var.vm_name}-nvme.qcow2" : ""
}

resource "null_resource" "nvme_disk" {
  count = var.nvme_enabled ? 1 : 0

  triggers = {
    disk_path = local.nvme_disk_path
    disk_size = var.nvme_disk_size
  }

  provisioner "local-exec" {
    interpreter = ["/bin/bash", "-c"]
    command = <<-EOT
      set -e
      DISK_PATH="${local.nvme_disk_path}"
      DISK_SIZE_BYTES=${var.nvme_disk_size}
      
      # Ensure build directory exists
      mkdir -p "$(dirname "$DISK_PATH")"
      
      # Convert bytes to MB for qemu-img
      DISK_SIZE_MB=$((DISK_SIZE_BYTES / 1024 / 1024))
      
      if [[ ! -f "$DISK_PATH" ]]; then
        echo "Creating NVMe disk: $DISK_PATH ($DISK_SIZE_MB MB)"
        qemu-img create -f qcow2 "$DISK_PATH" "$${DISK_SIZE_MB}M"
      else
        echo "NVMe disk already exists: $DISK_PATH"
      fi
      
      # Make readable by QEMU (runs as libvirt-qemu user)
      chmod 666 "$DISK_PATH"
    EOT
  }

  provisioner "local-exec" {
    when    = destroy
    command = "rm -f ${self.triggers.disk_path} 2>/dev/null || true"
  }
}

# Virtual machine domain
resource "libvirt_domain" "vm" {
  name        = var.vm_name
  memory      = var.memory_mb
  memory_unit = "MiB"
  vcpu        = var.vcpu_count
  type        = var.use_kvm ? "kvm" : "qemu"
  running     = true
  autostart   = true

  os = {
    type      = "hvm"
    type_arch = "x86_64"
    boot_devices = [
      { dev = "hd" }
    ]
  }

  cpu = {
    mode = var.use_kvm ? "host-passthrough" : "custom"
    # Use Nehalem for TCG emulation - provides SSE4.2 needed by SPDK/DPDK built with --target-arch=nehalem
    model = var.use_kvm ? null : "Nehalem"
  }

  devices = {
    disks = [
      {
        driver = {
          type = "qcow2"
        }
        source = {
          volume = {
            pool   = var.storage_pool
            volume = libvirt_volume.vm_disk.name
          }
        }
        target = {
          dev = "vda"
          bus = "virtio"
        }
      },
      {
        device = "cdrom"
        source = {
          volume = {
            pool   = var.storage_pool
            volume = libvirt_volume.cloudinit.name
          }
        }
        target = {
          dev = "sda"
          bus = "sata"
        }
        readonly = true
      }
    ]

    interfaces = [
      {
        model = {
          type = "virtio"
        }
        source = {
          network = {
            network = var.network_name
          }
        }
      }
    ]

    consoles = [
      {
        target = {
          type = "serial"
          port = 0
        }
      }
    ]

    graphics = [
      {
        vnc = {
          auto_port = true
        }
      }
    ]

    channels = [
      {
        target = {
          virt_io = {
            name = "org.qemu.guest_agent.0"
          }
        }
      }
    ]
  }
}

# Add NVMe device by patching domain XML (libvirt provider doesn't support NVMe natively)
# Uses QEMU command line passthrough to add NVMe controller and namespace
resource "null_resource" "add_nvme" {
  count = var.nvme_enabled ? 1 : 0

  triggers = {
    domain_id = libvirt_domain.vm.id
    nvme_path = local.nvme_disk_path
  }

  provisioner "local-exec" {
    command = <<-EOT
      set -e
      
      echo "Adding NVMe disk to VM ${var.vm_name}..."
      
      # Wait for VM to be defined
      for i in {1..30}; do
        if virsh dominfo ${var.vm_name} >/dev/null 2>&1; then
          break
        fi
        echo "Waiting for VM to be defined... ($i/30)"
        sleep 1
      done
      
      # Stop the VM if running
      if virsh list --name | grep -q "^${var.vm_name}$"; then
        echo "Stopping VM..."
        virsh destroy ${var.vm_name}
        sleep 2
      fi
      
      # Get current XML
      echo "Dumping VM XML..."
      virsh dumpxml ${var.vm_name} > /tmp/${var.vm_name}.xml
      
      # Check if NVMe already added (look for qemu:commandline with nvme)
      if grep -q 'drive id=nvme0' /tmp/${var.vm_name}.xml; then
        echo "NVMe disk already configured"
      else
        echo "Adding QEMU NVMe command line arguments..."
        
        # Add xmlns:qemu if not present
        if ! grep -q 'xmlns:qemu=' /tmp/${var.vm_name}.xml; then
          sed -i 's|<domain |<domain xmlns:qemu="http://libvirt.org/schemas/domain/qemu/1.0" |' /tmp/${var.vm_name}.xml
        fi
        
        # Add qemu:commandline section before </domain>
        # Use bus=pci.0,addr=0x10 to avoid conflict with video card on addr=0x2
        sed -i '/<\/domain>/i \
  <qemu:commandline>\
    <qemu:arg value="-drive"/>\
    <qemu:arg value="file=${local.nvme_disk_path},format=qcow2,if=none,id=nvme0"/>\
    <qemu:arg value="-device"/>\
    <qemu:arg value="nvme,serial=deadbeef,drive=nvme0,bus=pci.0,addr=0x10"/>\
  </qemu:commandline>' /tmp/${var.vm_name}.xml
        
        # Redefine domain
        echo "Redefining VM with NVMe..."
        virsh define /tmp/${var.vm_name}.xml
      fi
      
      # Start VM
      echo "Starting VM..."
      virsh start ${var.vm_name}
      
      rm -f /tmp/${var.vm_name}.xml
      echo "NVMe disk added successfully"
    EOT
    environment = {
      LIBVIRT_DEFAULT_URI = var.libvirt_uri
    }
  }

  depends_on = [libvirt_domain.vm, null_resource.nvme_disk]
}
