# dupefinder 2 electric boogaloo
Finds duplicate images from blockhash list

Very useful and much faster than the alternatives that i googled about 40 times faster than geeqie on truly large filesets. Compile this; download and compile C blockhash from http://blockhash.io.

Run blockhash with ``find . -regextype posix-egrep -regex ".*\.(png|jpe?g)$" -type f -printf '"%p"\n' | xargs -n1 -IF -P8 blockhash F >> hashes`` on your files.

Then run this program on hashes list, redirect output into a file and you'll get list of duplicates separated by newlines. You can bash then your way around the list, for example move all found files into something like dedup functionality in geeqie, and delete copies manually.
