use io_partition::clone_into_vec;
use std::error::Error;
use std::fmt;
use std::fmt::Display;
use std::io;
use std::io::{Read, Seek, SeekFrom};

#[derive(Debug)]
/// List all possible error that ``Sir0`` can return
pub enum Sir0Error {
    /// An error happened while performing an IO operation on a file
    IOError(io::Error),
    /// The magic of the Sir0 file does not correspond to what is expected
    InvalidMagic([u8; 4]),
    /// Impossible to create a partition of a file
    CreatePartitionError(io::Error),
    /// Impossible to clone a part of a file
    CloneHeaderError(io::Error),
}

impl Error for Sir0Error {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::IOError(err) | Self::CreatePartitionError(err) | Self::CloneHeaderError(err) => {
                Some(err)
            }
            Self::InvalidMagic(_) => None,
        }
    }
}

impl Display for Sir0Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IOError(_) => write!(f, "An error happened while performing an IO operation"),
            Self::InvalidMagic(magic) => write!(
                f,
                "The magic of the Sir0 file is not reconized: found {:?}",
                magic
            ),
            Self::CreatePartitionError(_) => {
                write!(f, "An error happened while creating a partition of a file")
            }
            Self::CloneHeaderError(_) => {
                write!(f, "An error happened while cloning a partition of a file")
            }
        }
    }
}

impl From<io::Error> for Sir0Error {
    fn from(err: io::Error) -> Sir0Error {
        Sir0Error::IOError(err)
    }
}

/// A Sir0 file, used in pokémon mystery dungeon on 3ds and DS (only tested with the 3ds version)
/// A Sir0 file contain a file, but have pointer to them.
#[derive(Debug)]
pub struct Sir0<T: Read + Seek> {
    offsets: Vec<u64>,
    header: Vec<u8>,
    file: T,
}

fn read_sir0_u32<T: Read>(file: &mut T) -> Result<u32, Sir0Error> {
    let mut buf = [0; 4];
    file.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

fn read_sir0_u8<T: Read>(file: &mut T) -> Result<u8, Sir0Error> {
    let mut buf = [0; 1];
    file.read_exact(&mut buf)?;
    Ok(u8::from_le_bytes(buf))
}

impl<T: Read + Seek> Sir0<T> {
    /// Create a new Sir0 from the file.
    pub fn new(mut file: T) -> Result<Self, Sir0Error> {
        file.seek(SeekFrom::Start(0))?;
        let mut magic = [0; 4];
        file.read_exact(&mut magic)?;
        if magic != [b'S', b'I', b'R', b'0'] {
            return Err(Sir0Error::InvalidMagic(magic));
        };

        let header_offset = read_sir0_u32(&mut file)?;
        let pointer_offset = read_sir0_u32(&mut file)?;

        let header_lenght = pointer_offset - header_offset;

        let header = clone_into_vec(&mut file, header_offset as u64, header_lenght as u64)
            .map_err(Sir0Error::CloneHeaderError)?;

        let file_lenght = file.seek(SeekFrom::End(0))?;
        file.seek(SeekFrom::Start(pointer_offset as u64))?;

        // just a rust translation of the code from evandixon
        let mut absolute_pointers = Vec::new();
        let mut is_constructing = false;
        let mut constructed_pointer: u64 = 0;
        let mut absolute_position: u64 = 0;
        for _ in 0..(file_lenght - (pointer_offset as u64) - 1) {
            let current = read_sir0_u8(&mut file)?;
            if current >= 128 {
                is_constructing = true;
                constructed_pointer = (constructed_pointer << 7) | ((current & 0x7F) as u64);
            } else if is_constructing {
                constructed_pointer = (constructed_pointer << 7) | ((current & 0x7F) as u64);
                absolute_position += constructed_pointer;
                absolute_pointers.push(absolute_position);
                is_constructing = false;
                constructed_pointer = 0;
            } else if current == 0 {
                break;
            } else {
                absolute_position += current as u64;
                absolute_pointers.push(absolute_position);
            }
        }

        Ok(Self {
            offsets: absolute_pointers,
            header,
            file,
        })
    }

    /// return the number of offsets this file contain
    pub fn offsets_len(&self) -> usize {
        self.offsets.len()
    }

    /// return the offset n°x. Offsets are in range `0..offsets_len`.
    pub fn offsets_get(&self, value: usize) -> Option<&u64> {
        self.offsets.get(value)
    }

    /// return the header of the file. It is independant of the Sir0 file format.
    pub fn get_header(&self) -> &Vec<u8> {
        &self.header
    }

    /// return the file contained in this sir0 file. It is the full sir0 file, with header and footer.
    pub fn get_file(&mut self) -> &mut T {
        &mut self.file
    }
}
