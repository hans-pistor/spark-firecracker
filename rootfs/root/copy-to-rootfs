ROOT_DIR=$1
if [ -z $ROOT_DIR ]
then
    echo "Missing argument: need a root directory"
    exit 1
fi
echo "Copying rootfs directories to $ROOT_DIR"
for d in bin etc lib root sbin usr; do tar c "/$d" | tar x -C $ROOT_DIR; done

# The above command may trigger the following message:
# tar: Removing leading "/" from member names
# However, this is just a warning, so you should be able to
# proceed with the setup process.

for dir in dev proc run sys var; do mkdir $ROOT_DIR/${dir}; done

# Delete this file from the mnt directory
rm $ROOT_DIR/root/copy-to-rootfs