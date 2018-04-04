# findr - finding files without flags

I am impressed by the sheer amount of functionality offered by the
Unix `find` command, but remain unable to remember how to use it
for anything other than the basics; otherwise I hit Google.
Obviously I don't have a very good memory for flags, but I do remember
_expressions_. `findr` is given exactly two arguments; the base directory
and a _filter expression_:

```
$ findr . 'path.ext=="rs" && path.size > 1kb'
$ findr . 'path.is_file && date.before("1 jan")'
$ findr . 'path.ext=="md" and date.after("last tuesday")'
```

The filter expression is passed `path` and `date` and fairly arbitrary
expressions are supported, thanks to the very capable little embedded
language [rhai](https://github.com/jonathandturner/rhai). As a little
convenience, "and", "or" and "not" are understood, since these are
easier to type in a hurry.

`path` has the following fields:

  - `is_file`   is this path a file?
  - `is_dir`    is this path a directory?
  - `size`      size of file entry in bytes
  - `ext`       extension of file path
  - `file_name` file name part of path

`date` has the following methods:

  - `before(datestr)`  all files modified before this date
  - `after(datestr)`   all files modified after this date
  - `between(datestr,datestr)`  all files modified between these dates

Numbers may have a size prefix (kb,mb,gb - not case-sensitive)
and date strings are interpreted by [chrono-english](https://github.com/stevedonovan/chrono-english).

No Flags Mini-philosophy:
Currently, `findr` ignores hidden directories, and speaks British English (i.e. not "9/11").
The idea is that such options will be passed as environment variables such as `FINDR_US`.


