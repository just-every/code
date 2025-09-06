# icy_sixel
For my projects I needed a rust sixel implementation. Unfortunately there aren't any.
There are several wrapper around the great libsixel library - but I do not want to struggle with another C dependency.

https://github.com/saitoha/libsixel

So I decided to just port the part I need to rust - and here it is.

I need it to save sixels in IcyDraw. I've my own sixel loading routines - but when I'm in the mood I'll port the library completely. I don't plan to support images in any way.
There are better libraries for rust available. Just the plain data.

Note: The dither stuff may be obsolete, for now it's in. Thanks to Hayaki Saito and all people who made libsixel possible.

It's likely that all other code paths that I use contain bugs so every feature need to be tested against the original libsixel. The original C code is very good & understandable so it's easy to extend from here. 

Contributions welcome - I just translated the minimum I need for my projects.

Code translated from libsixel revision 6a5be8b72d84037b83a5ea838e17bcf372ab1d5f