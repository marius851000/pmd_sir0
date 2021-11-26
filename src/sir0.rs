use byteorder::{ReadBytesExt, WriteBytesExt, LE};
use io_partition::clone_into_vec;
use std::io::Error as IOError;
use std::io::{Read, Seek, SeekFrom, Write};
use thiserror::Error;

#[derive(Debug, Error)]
/// List all possible error that ``Sir0`` can return
pub enum Sir0Error {
    #[error("An error happened while performing an IO operation")]
    IOError(#[from] IOError),
    #[error("The magic of the Sir0 file is not reconized: found {0:?}")]
    InvalidMagic([u8; 4]),
    #[error("An error happened while creating a partition of a file")]
    CreatePartitionError(#[source] IOError),
    #[error("An error happened while cloning a partition of a file")]
    CloneHeaderError(#[source] IOError),
    #[error("the sir0 file indicate that the pointer list of the file is at offset {1}, but that the header is at {0}, after the pointer list.")]
    PointerBeforeHeader(u32, u32),
    #[error("the offset of the pointer list ({0}) is too big: it is either past or at the end of file ({1})")]
    PointerOffsetPostOrAtFileEnd(u64, u64),
    #[error("the absolute position represented by the sir0 offset overflow the maximal capacity of an unsigned interget of 64 bit (absolute position: {0}, sum to add: {1}).")]
    AbsolutePointerOverflow(u64, u64),
}

/// A Sir0 file, used in pokémon mystery dungeon on 3ds and DS (only tested with the 3ds version)
/// A Sir0 file contain a file, but have pointer to them.
#[derive(Debug)]
pub struct Sir0<T: Read + Seek> {
    offsets: Vec<u64>,
    header: Vec<u8>,
    file: T,
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

        let header_offset = file.read_u32::<LE>()?;
        let pointer_offset = file.read_u32::<LE>()?;

        let header_lenght = pointer_offset.checked_sub(header_offset).map_or_else(
            || {
                Err(Sir0Error::PointerBeforeHeader(
                    header_offset,
                    pointer_offset,
                ))
            },
            Ok,
        )?;

        let header = clone_into_vec(&mut file, header_offset as u64, header_lenght as u64)
            .map_err(Sir0Error::CloneHeaderError)?;

        let file_lenght = file.seek(SeekFrom::End(0))?;
        file.seek(SeekFrom::Start(pointer_offset as u64))?;

        // just a rust translation of the code from evandixon
        let mut absolute_pointers = Vec::new();
        let mut is_constructing = false;
        let mut constructed_pointer: u64 = 0;
        let mut absolute_position: u64 = 0;
        let remaining_bytes = file_lenght
            .checked_sub(pointer_offset as u64)
            .map(|n| n.checked_sub(1))
            .flatten()
            .map_or_else(
                || {
                    Err(Sir0Error::PointerOffsetPostOrAtFileEnd(
                        pointer_offset as u64,
                        file_lenght,
                    ))
                },
                Ok,
            )?;
        for _ in 0..remaining_bytes {
            let current = file.read_u8()?;
            if current >= 128 {
                is_constructing = true;
                constructed_pointer =
                    constructed_pointer.overflowing_shl(7).0 | ((current & 0x7F) as u64);
            } else if is_constructing {
                constructed_pointer =
                    constructed_pointer.overflowing_shl(7).0 | ((current & 0x7F) as u64);
                absolute_position = absolute_position
                    .checked_add(constructed_pointer)
                    .map_or_else(
                        || {
                            Err(Sir0Error::AbsolutePointerOverflow(
                                absolute_position,
                                constructed_pointer,
                            ))
                        },
                        Ok,
                    )?;
                absolute_pointers.push(absolute_position);
                is_constructing = false;
                constructed_pointer = 0;
            } else if current == 0 {
                break;
            } else {
                absolute_position = absolute_position.checked_add(current as u64).map_or_else(
                    || {
                        Err(Sir0Error::AbsolutePointerOverflow(
                            absolute_position,
                            current as u64,
                        ))
                    },
                    Ok,
                )?;
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

/// write the sir0 header at the current position of the file. It should be written at the beggining of the file, but require to know the header and offset list offset.
///
/// It have a constant size of 12 bytes, so you should reserve 12 bytes at the beggining of the file, write it, write the header at the end of it, call [`write_sir0_footer`],
/// seek at the beggining of the file and call this function.
pub fn write_sir0_header(
    file: &mut impl Write,
    header_offset: u32,
    offset_offset: u32,
) -> Result<(), IOError> {
    file.write_all(&[b'S', b'I', b'R', b'0'])?;
    file.write_u32::<LE>(header_offset)?;
    file.write_u32::<LE>(offset_offset)?;
    Ok(())
}

/// An error that occured while writing a sir0 footer
#[derive(Error, Debug)]
pub enum Sir0WriteFooterError {
    #[error("an error occured while writing the file")]
    IOError(#[from] IOError),
    #[error("an element in the isn't sorted nicely. They need to be smaller from the bigger to the biggest. ( {0} is bigger than {1}")]
    NotSorted(u32, u32),
}

/// Write a sir0 footer, pointing to the various element in the list.
/// The element of the list is based on the posititon since the start of the file. For a normal Sir0 file, the first 2 element should be [4, 8]
pub fn write_sir0_footer<T>(file: &mut T, list: &[u32]) -> Result<(), Sir0WriteFooterError>
where
    T: Write,
{
    let mut latest_written_pointer = 0;
    for original_to_write in list.to_owned() {
        let mut remaining_to_write = original_to_write
            .checked_sub(latest_written_pointer)
            .map_or_else(
                || {
                    Err(Sir0WriteFooterError::NotSorted(
                        original_to_write,
                        latest_written_pointer,
                    ))
                },
                Ok,
            )?;
        latest_written_pointer = original_to_write;
        let mut reversed_to_write = Vec::new();
        if remaining_to_write == 0 {
            continue
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
