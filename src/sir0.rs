use io_partition::clone_into_vec;
use std::error::Error;
use std::fmt;
use std::fmt::Display;
use std::io::{Read, Seek, Write, SeekFrom};
use std::io::Error as IOError;

#[derive(Debug)]
/// List all possible error that ``Sir0`` can return
pub enum Sir0Error {
    /// An error happened while performing an IO operation on a file
    IOError(IOError),
    /// The magic of the Sir0 file does not correspond to what is expected
    InvalidMagic([u8; 4]),
    /// Impossible to create a partition of a file
    CreatePartitionError(IOError),
    /// Impossible to clone a part of a file
    CloneHeaderError(IOError),
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

impl From<IOError> for Sir0Error {
    fn from(err: IOError) -> Sir0Error {
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

/// Write a sir0 footer, pointing to the various element in the list.
/// The element of the list is based on the posititon since the start of the file. For a normal Sir0 file, the first 2 element should be [4, 8]
#[allow(dead_code)]
pub fn write_sir0_footer<T>(file: &mut T, list: Vec<u32>) -> Result<(), IOError>
where
    T: Write,
{
    let mut latest_written_pointer = 0;
    for original_to_write in list {
        let mut remaining_to_write = original_to_write - latest_written_pointer;
        latest_written_pointer = original_to_write;
        let mut reversed_to_write = Vec::new();
        if remaining_to_write == 0 {
            //NOTE: this never happen in original game. This is an extrapolation of what will need to be written in such a situation.
            reversed_to_write.push(0);
        } else {
            loop {
                if remaining_to_write >= 128 {
                    let to_write = (remaining_to_write % 128) as u8;
                    remaining_to_write >>= 7;
                    reversed_to_write.push(to_write);
                } else {
                    reversed_to_write.push(remaining_to_write as u8);
                    break;
                }
            }
        }
        for (counter, value_to_write) in reversed_to_write.iter().cloned().enumerate().rev() {
            if counter == 0 {
                file.write_all(&[value_to_write])?;
            } else {
                file.write_all(&[value_to_write + 0b1000_0000])?;
            }
        }
    }
    Ok(())
}
