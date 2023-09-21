FROM alpine

WORKDIR /root

# Set root password
RUN echo "root:root" | chpasswd

# Download dependencies
RUN apk add openrc util-linux python3 curl

# Setup login terminal on ttyS0
RUN ln -s agetty /etc/init.d/agetty.ttyS0 \
    && echo ttyS0 > /etc/securetty \
    && rc-update add agetty.ttyS0 default

# Make sure special file systems are mounted on boot
RUN rc-update add devfs boot \
    && rc-update add procfs boot \
    && rc-update add sysfs boot

COPY etc /etc
COPY root /root
copy usr /usr