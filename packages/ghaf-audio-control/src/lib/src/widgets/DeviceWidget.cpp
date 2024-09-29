/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#include <GhafAudioControl/widgets/DeviceWidget.hpp>

#include <gtkmm/adjustment.h>

namespace ghaf::AudioControl
{

namespace
{

constexpr auto ScaleSize = 200;
constexpr auto ScaleOrientation = Gtk::Orientation::ORIENTATION_HORIZONTAL;
constexpr auto ScaleInitialValue = 0.0;
constexpr auto ScaleLowerLimit = 0.0;
constexpr auto ScaleUpperLimit = 100.0;

constexpr auto DeviceWidgetSpacing = 5;

auto Bind(const auto& appProp, const auto& widgetProp, bool readonly = false)
{
    auto flag = Glib::BindingFlags::BINDING_SYNC_CREATE;
    if (!readonly)
        flag |= Glib::BindingFlags::BINDING_BIDIRECTIONAL;

    return Glib::Binding::bind_property(appProp, widgetProp, flag);
}

Gtk::Scale* MakeScaleWidget()
{
    auto adjustment = Gtk::Adjustment::create(ScaleInitialValue, ScaleLowerLimit, ScaleUpperLimit);

    auto* scale = Gtk::make_managed<Gtk::Scale>(std::move(adjustment), ScaleOrientation);
    scale->set_size_request(ScaleSize);
    scale->set_digits(0);

    return scale;
}

} // namespace

DeviceWidget::DeviceWidget(DeviceModel::Ptr model)
    : Gtk::Box(Gtk::ORIENTATION_HORIZONTAL)
    , m_model(std::move(model))
    , m_nameLabel(Gtk::make_managed<Gtk::Label>())
    , m_switch(Gtk::make_managed<Gtk::Switch>())
    , m_scale(MakeScaleWidget())
    , m_bindings({Bind(m_model->getNameProperty(), m_nameLabel->property_label(), true),
                  Bind(m_model->getSoundVolumeProperty(), m_scale->get_adjustment()->property_value()),
                  Bind(m_model->getSoundEnabledProperty(), m_switch->property_state())})
{
    const auto setup = [](Gtk::Widget& widget)
    {
        widget.set_hexpand(false);
        widget.set_vexpand(false);
        widget.set_halign(Gtk::Align::ALIGN_START);
        widget.set_valign(Gtk::Align::ALIGN_END);
    };

    set_homogeneous(true);
    set_spacing(DeviceWidgetSpacing);

    setup(*m_nameLabel);
    setup(*m_switch);
    setup(*m_scale);

    add(*m_nameLabel);
    add(*m_switch);
    add(*m_scale);

    m_nameLabel->set_can_focus(false);
    m_nameLabel->set_selectable(false);
    m_nameLabel->set_focus_on_click(false);

    m_switch->set_halign(Gtk::Align::ALIGN_END);
    m_scale->set_halign(Gtk::Align::ALIGN_END);

    set_valign(Gtk::ALIGN_START);
    show_all_children();
}

} // namespace ghaf::AudioControl
