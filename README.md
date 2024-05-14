# vju
A widget for displaying arbitrary text piped in from stdin  
When used as selection tool it outputs the user choice to stdout

## Installation
### Install vju
### Install vju from source
```
$ git clone https://github.com/bbusse/vju
$ cd vju
$ cargo install --path .
```
## Build
### Build
```
$ git clone https://github.com/bbusse/vju
$ cd vju
$ cargo build --release
```
### Run Build
```
$ cargo run --release
```
## How
vju reads from stdin and uses egui to draw the widget containing the received text

# TODO
Search / Filter  
Input Selection / Choice  
Scrolling / Paging

# Resources
[egui](https://github.com/emilk/egui)  
