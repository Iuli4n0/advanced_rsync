# advanced_rsync
Simple rust application that lets you synchronise files between local folders, ftp locations and zip files.<br />
The app runs continuously and keeps the two locations synchronized. Specifically:<br />
If a file is created in one location, the file is duplicated in the other location.<br />
If a file is deleted from one location, it is also deleted from the other location.<br />
If a file is modified in one location, the modification is copied to the other location.<br />
<br />
Upon initial startup, synchronization is performed as follows:<br />
If a file exists only in one location, it is copied to the other location.<br />
If the same file exists in both locations but there are differences, the newest version is copied.<br />
zip archives are treated as read only, only ftp and folders can change.

