//! Formatting for log records.
//! 
//! This module contains a [`Formatter`] that can be used to format log records
//! into without needing temporary allocations. Usually you won't need to worry
//! about the contents of this module and can use the `Formatter` like an ordinary
//! [`Write`].
//! 
//! # Formatting log records
//! 
//! The format used to print log records can be customised using the [`Builder::format`]
//! method.
//! Custom formats can apply different color and weight to printed values using
//! [`Style`] builders.
//! 
//! ```
//! use std::io::Write;
//! use env_logger::fmt::Color;
//! 
//! let mut builder = env_logger::Builder::new();
//! 
//! builder.format(|buf, record| {
//!     let mut level_style = buf.style();
//! 
//!     level_style.set_color(Color::Red).set_bold(true);
//! 
//!     writeln!(buf, "{}: {}",
//!         level_style.value(record.level()),
//!         record.args())
//! });
//! ```
//! 
//! [`Formatter`]: struct.Formatter.html
//! [`Style`]: struct.Style.html
//! [`Builder::format`]: ../struct.Builder.html#method.format
//! [`Write`]: https://doc.rust-lang.org/stable/std/io/trait.Write.html

use std::io::prelude::*;
use std::{io, fmt};
use std::rc::Rc;
use std::cell::RefCell;

use termcolor::{ColorSpec, ColorChoice, Buffer, BufferWriter, WriteColor};
use chrono::{DateTime, Utc};
use chrono::format::Item;

pub use termcolor::Color;

/// A formatter to write logs into.
/// 
/// `Formatter` implements the standard [`Write`] trait for writing log records.
/// It also supports terminal colors, through the [`style`] method.
/// 
/// # Examples
/// 
/// Use the [`writeln`] macro to easily format a log record:
/// 
/// ```
/// use std::io::Write;
/// 
/// let mut builder = env_logger::Builder::new();
/// 
/// builder.format(|buf, record| writeln!(buf, "{}: {}", record.level(), record.args()));
/// ```
/// 
/// [`Write`]: https://doc.rust-lang.org/stable/std/io/trait.Write.html
/// [`writeln`]: https://doc.rust-lang.org/stable/std/macro.writeln.html
/// [`style`]: #method.style
pub struct Formatter {
    buf: Rc<RefCell<Buffer>>,
    write_style: WriteStyle,
}

/// A set of styles to apply to the terminal output.
/// 
/// Call [`Formatter::style`] to get a `Style` and use the builder methods to 
/// set styling properties, like [color] and [weight].
/// To print a value using the style, wrap it in a call to [`value`] when the log
/// record is formatted.
/// 
/// # Examples
/// 
/// Create a bold, red colored style and use it to print the log level:
/// 
/// ```
/// use std::io::Write;
/// use env_logger::fmt::Color;
/// 
/// let mut builder = env_logger::Builder::new();
/// 
/// builder.format(|buf, record| {
///     let mut level_style = buf.style();
/// 
///     level_style.set_color(Color::Red).set_bold(true);
/// 
///     writeln!(buf, "{}: {}",
///         level_style.value(record.level()),
///         record.args())
/// });
/// ```
/// 
/// Styles can be re-used to output multiple values:
/// 
/// ```
/// use std::io::Write;
/// use env_logger::fmt::Color;
/// 
/// let mut builder = env_logger::Builder::new();
/// 
/// builder.format(|buf, record| {
///     let mut bold = buf.style();
/// 
///     bold.set_bold(true);
/// 
///     writeln!(buf, "{}: {} {}",
///         bold.value(record.level()),
///         bold.value("some bold text"),
///         record.args())
/// });
/// ```
/// 
/// [`Formatter::style`]: struct.Formatter.html#method.style
/// [color]: #method.set_color
/// [weight]: #method.set_bold
/// [`value`]: #method.value
#[derive(Clone)]
pub struct Style {
    buf: Rc<RefCell<Buffer>>,
    spec: ColorSpec,
}

/// A value that can be printed using the given styles.
/// 
/// It is the result of calling [`Style::value`].
/// 
/// [`Style::value`]: struct.Style.html#method.value
pub struct StyledValue<'a, T> {
    style: &'a Style,
    value: T,
}

/// An [RFC3339] formatted timestamp.
/// 
/// The timestamp implements [`Display`] and can be written to a [`Formatter`].
/// 
/// [RFC3339]: https://www.ietf.org/rfc/rfc3339.txt
/// [`Display`]: https://doc.rust-lang.org/stable/std/fmt/trait.Display.html
/// [`Formatter`]: struct.Formatter.html
pub struct Timestamp(DateTime<Utc>);

/// Log target, either `stdout` or `stderr`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Target {
    /// Logs will be sent to standard output.
    Stdout,
    /// Logs will be sent to standard error.
    Stderr,
}

impl Default for Target {
    fn default() -> Self {
        Target::Stderr
    }
}

/// Whether or not to print styles to the target.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WriteStyle {
    /// Try to print styles, but don't force the issue.
    Auto,
    /// Try very hard to print styles.
    Always,
    /// Never print styles.
    Never,
}

impl Default for WriteStyle {
    fn default() -> Self {
        WriteStyle::Auto
    }
}

/// A terminal target with color awareness.
pub(crate) struct Writer {
    inner: BufferWriter,
    write_style: WriteStyle,
}

impl Writer {
    pub(crate) fn write_style(&self) -> WriteStyle {
        self.write_style
    }
}

/// A builder for a terminal writer.
/// 
/// The target and style choice can be configured before building.
pub(crate) struct Builder {
    target: Target,
    write_style: WriteStyle,
}

impl Builder {
    /// Initialize the writer builder with defaults.
    pub fn new() -> Self {
        Builder {
            target: Default::default(),
            write_style: Default::default(),
        }
    }

    /// Set the target to write to.
    pub fn target(&mut self, target: Target) -> &mut Self {
        self.target = target;
        self
    }

    /// Parses a style choice string.
    /// 
    /// See the [Disabling colors] section for more details.
    /// 
    /// [Disabling colors]: ../index.html#disabling-colors
    pub fn parse(&mut self, write_style: &str) -> &mut Self {
        self.write_style(parse_write_style(write_style))
    }

    /// Whether or not to print style characters when writing.
    pub fn write_style(&mut self, write_style: WriteStyle) -> &mut Self {
        self.write_style = write_style;
        self
    }

    /// Build a terminal writer.
    pub fn build(&mut self) -> Writer {
        let color_choice = match self.write_style {
            WriteStyle::Auto => ColorChoice::Auto,
            WriteStyle::Always => ColorChoice::Always,
            WriteStyle::Never => ColorChoice::Never,
        };

        let writer = match self.target {
            Target::Stderr => BufferWriter::stderr(color_choice),
            Target::Stdout => BufferWriter::stdout(color_choice),
        };

        Writer {
            inner: writer,
            write_style: self.write_style,
        }
    }
}

impl Default for Builder {
    fn default() -> Self {
        Builder::new()
    }
}

impl Style {
    /// Set the text color.
    /// 
    /// # Examples
    /// 
    /// Create a style with red text:
    /// 
    /// ```
    /// use std::io::Write;
    /// use env_logger::fmt::Color;
    /// 
    /// let mut builder = env_logger::Builder::new();
    /// 
    /// builder.format(|buf, record| {
    ///     let mut style = buf.style();
    /// 
    ///     style.set_color(Color::Red);
    /// 
    ///     writeln!(buf, "{}", style.value(record.args()))
    /// });
    /// ```
    pub fn set_color(&mut self, color: Color) -> &mut Style {
        self.spec.set_fg(Some(color));
        self
    }

    /// Set the text weight.
    /// 
    /// If `yes` is true then text will be written in bold.
    /// If `yes` is false then text will be written in the default weight.
    /// 
    /// # Examples
    /// 
    /// Create a style with bold text:
    /// 
    /// ```
    /// use std::io::Write;
    /// 
    /// let mut builder = env_logger::Builder::new();
    /// 
    /// builder.format(|buf, record| {
    ///     let mut style = buf.style();
    /// 
    ///     style.set_bold(true);
    /// 
    ///     writeln!(buf, "{}", style.value(record.args()))
    /// });
    /// ```
    pub fn set_bold(&mut self, yes: bool) -> &mut Style {
        self.spec.set_bold(yes);
        self
    }

    /// Set the background color.
    /// 
    /// # Examples
    /// 
    /// Create a style with a yellow background:
    /// 
    /// ```
    /// use std::io::Write;
    /// use env_logger::fmt::Color;
    /// 
    /// let mut builder = env_logger::Builder::new();
    /// 
    /// builder.format(|buf, record| {
    ///     let mut style = buf.style();
    /// 
    ///     style.set_bg(Color::Yellow);
    /// 
    ///     writeln!(buf, "{}", style.value(record.args()))
    /// });
    /// ```
    pub fn set_bg(&mut self, color: Color) -> &mut Style {
        self.spec.set_bg(Some(color));
        self
    }

    /// Wrap a value in the style.
    /// 
    /// The same `Style` can be used to print multiple different values.
    /// 
    /// # Examples
    /// 
    /// Create a bold, red colored style and use it to print the log level:
    /// 
    /// ```
    /// use std::io::Write;
    /// use env_logger::fmt::Color;
    /// 
    /// let mut builder = env_logger::Builder::new();
    /// 
    /// builder.format(|buf, record| {
    ///     let mut style = buf.style();
    /// 
    ///     style.set_color(Color::Red).set_bold(true);
    /// 
    ///     writeln!(buf, "{}: {}",
    ///         style.value(record.level()),
    ///         record.args())
    /// });
    /// ```
    pub fn value<T>(&self, value: T) -> StyledValue<T> {
        StyledValue {
            style: &self,
            value
        }
    }
}

impl Formatter {
    pub(crate) fn new(writer: &Writer) -> Self {
        Formatter {
            buf: Rc::new(RefCell::new(writer.inner.buffer())),
            write_style: writer.write_style(),
        }
    }

    pub(crate) fn write_style(&self) -> WriteStyle {
        self.write_style
    }

    /// Begin a new [`Style`].
    /// 
    /// # Examples
    /// 
    /// Create a bold, red colored style and use it to print the log level:
    /// 
    /// ```
    /// use std::io::Write;
    /// use env_logger::fmt::Color;
    /// 
    /// let mut builder = env_logger::Builder::new();
    /// 
    /// builder.format(|buf, record| {
    ///     let mut level_style = buf.style();
    /// 
    ///     level_style.set_color(Color::Red).set_bold(true);
    /// 
    ///     writeln!(buf, "{}: {}",
    ///         level_style.value(record.level()),
    ///         record.args())
    /// });
    /// ```
    /// 
    /// [`Style`]: struct.Style.html
    pub fn style(&self) -> Style {
        Style {
            buf: self.buf.clone(),
            spec: ColorSpec::new(),
        }
    }

    /// Get a [`Timestamp`] for the current date and time in UTC.
    /// 
    /// # Examples
    /// 
    /// Include the current timestamp with the log record:
    /// 
    /// ```
    /// use std::io::Write;
    /// 
    /// let mut builder = env_logger::Builder::new();
    /// 
    /// builder.format(|buf, record| {
    ///     let ts = buf.timestamp();
    /// 
    ///     writeln!(buf, "{}: {}: {}", ts, record.level(), record.args())
    /// });
    /// ```
    /// 
    /// [`Timestamp`]: struct.Timestamp.html
    pub fn timestamp(&self) -> Timestamp {
        Timestamp(Utc::now())
    }

    pub(crate) fn print(&self, writer: &Writer) -> io::Result<()> {
        writer.inner.print(&self.buf.borrow())
    }

    pub(crate) fn clear(&mut self) {
        self.buf.borrow_mut().clear()
    }
}

impl Write for Formatter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buf.borrow_mut().write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.buf.borrow_mut().flush()
    }
}

impl<'a, T> StyledValue<'a, T> {
    fn write_fmt<F>(&self, f: F) -> fmt::Result
    where
        F: FnOnce() -> fmt::Result,
    {
        self.style.buf.borrow_mut().set_color(&self.style.spec).map_err(|_| fmt::Error)?;

        // Always try to reset the terminal style, even if writing failed
        let write = f();
        let reset = self.style.buf.borrow_mut().reset().map_err(|_| fmt::Error);

        write.and(reset)
    }
}

impl fmt::Debug for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        /// A `Debug` wrapper for `Timestamp` that uses the `Display` implementation.
        struct TimestampValue<'a>(&'a Timestamp);

        impl<'a> fmt::Debug for TimestampValue<'a> {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }

        f.debug_tuple("Timestamp")
         .field(&TimestampValue(&self))
         .finish()
    }
}

impl fmt::Debug for Writer {
    fn fmt(&self, f: &mut fmt::Formatter)->fmt::Result {
        f.debug_struct("Writer").finish()
    }
}

impl fmt::Debug for Formatter {
    fn fmt(&self, f: &mut fmt::Formatter)->fmt::Result {
        f.debug_struct("Formatter").finish()
    }
}

impl fmt::Debug for Builder {
    fn fmt(&self, f: &mut fmt::Formatter)->fmt::Result {
        f.debug_struct("Logger")
        .field("target", &self.target)
        .field("write_style", &self.write_style)
        .finish()
    }
}

impl fmt::Debug for Style {
    fn fmt(&self, f: &mut fmt::Formatter)->fmt::Result {
        f.debug_struct("Style").field("spec", &self.spec).finish()
    }
}

macro_rules! impl_styled_value_fmt {
    ($($fmt_trait:path),*) => {
        $(
            impl<'a, T: $fmt_trait> $fmt_trait for StyledValue<'a, T> {
                fn fmt(&self, f: &mut fmt::Formatter)->fmt::Result {
                    self.write_fmt(|| T::fmt(&self.value, f))
                }
            }
        )*
    };
}

impl_styled_value_fmt!(
    fmt::Debug,
    fmt::Display,
    fmt::Pointer,
    fmt::Octal,
    fmt::Binary,
    fmt::UpperHex,
    fmt::LowerHex,
    fmt::UpperExp,
    fmt::LowerExp);

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter)->fmt::Result {
        const ITEMS: &'static [Item<'static>] = {
            use chrono::format::Item::*;
            use chrono::format::Numeric::*;
            use chrono::format::Fixed::*;
            use chrono::format::Pad::*;

            &[
                Numeric(Year, Zero),
                Literal("-"),
                Numeric(Month, Zero),
                Literal("-"),
                Numeric(Day, Zero),
                Literal("T"),
                Numeric(Hour, Zero),
                Literal(":"),
                Numeric(Minute, Zero),
                Literal(":"),
                Numeric(Second, Zero),
                Fixed(TimezoneOffsetZ),
            ]
        };

        self.0.format_with_items(ITEMS.iter().cloned()).fmt(f)
    }
}

fn parse_write_style(spec: &str) -> WriteStyle {
    match spec {
        "auto" => WriteStyle::Auto,
        "always" => WriteStyle::Always,
        "never" => WriteStyle::Never,
        _ => Default::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_write_style_valid() {
        let inputs = vec![
            ("auto", WriteStyle::Auto),
            ("always", WriteStyle::Always),
            ("never", WriteStyle::Never),
        ];

        for (input, expected) in inputs {
            assert_eq!(expected, parse_write_style(input));
        }
    }

    #[test]
    fn parse_write_style_invalid() {
        let inputs = vec![
            "",
            "true",
            "false",
            "NEVER!!"
        ];

        for input in inputs {
            assert_eq!(WriteStyle::Auto, parse_write_style(input));
        }
    }
}
