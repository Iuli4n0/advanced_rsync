# advanced_rsync
Simple rust application that lets you synchronise files between local folders, ftp locations and zip files.__
The app runs continuously and keeps the two locations synchronized. Specifically:__
If a file is created in one location, the file is duplicated in the other location.__
If a file is deleted from one location, it is also deleted from the other location.__
If a file is modified in one location, the modification is copied to the other location.__
__
Upon initial startup, synchronization is performed as follows:__
If a file exists only in one location, it is copied to the other location.__
If the same file exists in both locations but there are differences, the newest version is copied.__
zip archives are treated as read only, only ftp and folders can change.

