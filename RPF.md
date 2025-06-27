**RPF** (**RAGE Package File**) is the format of [[Archive|game archive]]s used in [[RAGE Engine]]. They can be edited with [[SparkIV]] or [[OpenIV]].
[[File:Rockstar_RPF_Viewer.png|right|thumb|300px|The internal software used by Rockstar to view these files]]

## RPF Versions

The RPF Version tells us the version of the RPF Archive:

- Version 0: 0x52504630 - Rockstar Games Presents Table Tennis
- Version 2: 0x52504632 - Grand Theft Auto IV
- Version 3: 0x52504633 - Grand Theft Auto IV Audio & Midnight Club: Los Angeles
- Version 4: 0x52504634 - Max Payne 3
- Version 6: 0x52504636 - Red Dead Redemption
- Version 7: 0x52504637 - Grand Theft Auto V
- Version 8: 0x52504638 - Red Dead Redemption 2

## RPF Version 0

### Header

The RPF files all contain the same header. The header tells us the version of the archive and how many entries there are.

- 4b - INT32 - RPF Version
- 4b - INT32 - Table of Contents Size (in bytes)
- 4b - INT32 - Number of Entries (root entry '/' included)

The Table of Contents Size tells us the size of the table of contents (toc) segment in the file.  
The Number of Entries tells us the number of files contained in the archive.

### Table of Contents

The Table of Contents size is determined by the header of the RPF archive. It starts after 2048 bytes from the file origin. The Table of Contents contains both Directories and File Entries. One entry takes up 16 bytes. The minimum toc size is 2048 bytes.

#### Directory Entry

Directory entries follow this structure:

- 3b - INT24 - Name Offset
- 1b - bool - Entry type ("80" if it is a directory / 0 if it is a file)
- 4b - UINT32 - First toc filesystem-entry entry Index in the directory (zero-based)
- 4b - UINT32 - Count of filesystem-entries in the directory
- 4b - UINT32 - Count of filesystem-entries in the directory (identical, probably to keep the structure to 16 bytes per entry)

**Name offset**: Stores the offset of the first char of the name of the filesystem-entry within the filename-section.  
**First toc filesystem-entry entry Index in the directory**: Folders can contain either other folders or files. Here the offset of the first object within the directory is stored.

#### File Entry

The File entries follow this structure:

- 3b - INT24 - Name Offset
- 1b - bool - Entry type ("80" if it is a directory / 0 if it is a file)
- 4b - INT32 - Offset (zero-based)
- 4b - INT32 - Size (bytes)
- 4b - INT32 - Uncompressed size (bytes)

**Name offset**: Stores the offset of the first char of the name of the filesystem-entry within the filename-section.  
**Offset**: Offset of the data of the file within the RPF (zero-based).  
**Size**: Size of the file within the RPF (compressed).  
**Uncompressed size**: Size of the file after decompression.

Note that the file is stored uncompressed within the RPF if Size equals the Uncompressed size.  
The files are always compressed using zlib without a deflate header.

## Filename-Section

The name section starts after 2048+(16*number of toc entries) bytes. The file names are separated with "00".

### Tools

The following tools can be used to import & export files into RPF archives:

- [[OpenIV]]
- [[SparkIV]]
- [CodeWalker](https://www.gta5-mods.com/tools/codewalker-gtav-interactive-3d-map)

## RPF Version 2

### Header

The RPF files all contain the same header. The header tells us the version of the archive, how many entries there are, and whether or not the archive is encrypted.

- 4b - INT32 - RPF Version
- 4b - INT32 - Table of Contents Size
- 4b - INT32 - Number of Entries
- 4b - INT32 - Unknown
- 4b - INT32 - Encrypted

The Table of Contents Size tells us the size of the table of contents segment in the file.  
The Number of Entries tells us the number of files contained in the archive.  
The Encryption flag tells us if the archive is encrypted; if the archive is unencrypted, it is set to 0, otherwise it is non-zero.

### Table of Contents

The Table of Contents size is determined by the Table of Contents Size integer in the Header of the RPF archive. It starts after 2048 bytes from the file origin, and is encrypted depending on the encryption flag in the header. The Table of Contents contains both Directories and File Entries. Both are different structures and take up different amounts of space.

#### Directory Entry

Directory entries follow this structure:

- 4b - INT32 - Name Offset
- 4b - INT32 - Flags
- 4b - UINT32 - Content Entry Index
- 4b - UINT32 - Content Entry Count

**Name Offset**: Refers to the file offset that stores the name of the directory.  
**Flags**: Just give information about the directory.  
**Content Entry Index**: Generally described in the first bit of the unsigned integer.  
**Content Entry Count**: Describes how many entries are under this directory, and is generally defined in the first 4 bits of the unsigned integer.

#### File Entry

The File entries follow this structure:

- 4b - INT32 - Name Offset
- 4b - INT32 - Size
- 3b - UINT24 - Offset
- 1b - UCHAR8 - Resource Type
- 4b - UINT32 - Flags

**Name Offset**: Refers to the file offset that stores the name of the file.  
**Size**: Tells us the size of the file.  
**Offset**: Tells us the file offset the file is stored in.  
**Resource Type**: Tells us the [[Resource_Type|type of resource]] that the file is.  
In the resource flag, the first bit tells us whether the file is compressed or not.

{{Incomplete}}

### Tools

The following tools can be used to import & export files into RPF archives:

- [[OpenIV]]
- [[SparkIV]]

{{N|5|4}}
