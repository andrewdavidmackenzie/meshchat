use crate::device_view::DeviceView;
use crate::device_view::DeviceViewMessage::{MessageInput, SendMessage};
use crate::Message;
use iced::widget::scrollable::Scrollbar;
use iced::widget::{scrollable, text, text_input, Column, Row};
use iced::{Element, Fill};
use meshtastic::protobufs::mesh_packet::PayloadVariant::Decoded;
use meshtastic::protobufs::{MeshPacket, PortNum};

// Make this a struct and move the message field in here
pub fn channel_view(device_view: &DeviceView, packets: &[MeshPacket]) -> Element<'static, Message> {
    let mut channel_view = Column::new();

    for packet in packets {
        if let Some(Decoded(data)) = &packet.payload_variant
            && data.emoji == 0
        {
            match PortNum::try_from(data.portnum) {
                Ok(PortNum::TextMessageApp) => {
                    // false - TODO handle emoji replies
                    let mut packet_row = Row::new();
                    packet_row = packet_row.push(
                        text(String::from_utf8(data.payload.clone()).unwrap())
                            .shaping(text::Shaping::Advanced),
                    );
                    channel_view = channel_view.push(packet_row);
                }
                Ok(PortNum::PositionApp) => println!("Position payload"),
                Ok(PortNum::AlertApp) => println!("Alert payload"),
                Ok(PortNum::TelemetryApp) => println!("Telemetry payload"),
                Ok(PortNum::NeighborinfoApp) => println!("Neighbor Info payload"),
                Ok(PortNum::NodeinfoApp) => println!("Node Info payload"),
                _ => eprintln!("Unknown portnum: {}", data.portnum),
            }
        }
    }

    let channel_scroll = scrollable(channel_view)
        .direction({
            let scrollbar = Scrollbar::new().width(10.0);
            scrollable::Direction::Vertical(scrollbar)
        })
        .width(Fill)
        .height(Fill);

    // TODO set an icon,
    let text_box = text_input("Message>", &device_view.message)
        .on_input(|s| Message::Device(MessageInput(s)))
        .on_submit(Message::Device(SendMessage));
    let bottom_row = Row::new().push(text_box);

    Column::new().push(channel_scroll).push(bottom_row).into()
}
