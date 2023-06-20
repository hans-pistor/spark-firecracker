set -eu

SCRIPT_DIRECTORY=$(dirname "$0")
ARCH="$(uname -m)"
KERNEL_BOOT_ARGS="console=ttyS0 reboot=k panic=1 pci=off"

# Make sure linux kernel is available
[ -e /tmp/vmlinux.bin ] || wget -P /tmp https://s3.amazonaws.com/spec.ccfc.min/img/quickstart_guide/${ARCH}/kernels/vmlinux.bin

# build the rootfs
$SCRIPT_DIRECTORY/create-rootfs


# Some networking setup
TAP_DEVICE="tap0"
HOST_IFACE="ens33"

sudo ip link del $TAP_DEVICE 2> /dev/null || true
sudo ip tuntap add $TAP_DEVICE mode tap
sudo ip addr add 172.16.0.1/24 dev $TAP_DEVICE
sudo ip link set $TAP_DEVICE up
sudo sh -c "echo 1 > /proc/sys/net/ipv4/ip_forward"
sudo iptables -t nat -A POSTROUTING -o $HOST_IFACE -j MASQUERADE
sudo iptables -A FORWARD -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT
sudo iptables -A FORWARD -i $TAP_DEVICE -o $HOST_IFACE -j ACCEPT


cat <<EOF > /tmp/vmconfig.json
{
  "boot-source": {
    "kernel_image_path": "vmlinux.bin",
    "boot_args": "$KERNEL_BOOT_ARGS"
  },
  "drives": [
    {
      "drive_id": "rootfs",
      "path_on_host": "/tmp/rootfs.ext4",
      "is_root_device": true,
      "is_read_only": false
    }
  ],
  "network-interfaces": [
      {
          "iface_id": "eth0",
          "guest_mac": "AA:FC:00:00:00:01",
          "host_dev_name": "$TAP_DEVICE"
      }
  ]
}
EOF
/home/hpistor/firecracker/build/cargo_target/${ARCH}-unknown-linux-musl/debug/firecracker --no-api --config-file /tmp/vmconfig.json